//! OIDC JWT verification and token decoding.

use std::collections::HashMap;

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
};
use serde::{Deserialize, Serialize};

use domain::auth::user::UserId;
use domain::common::{DisplayName, Email};

use crate::claims::AuthenticatedUser;
use crate::error::AuthError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Configuration parameters for OpenID Connect token validation.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OidcConfig {
    issuer: Option<String>,
    audience: Option<String>,
    hmac_secret: Option<String>,
    rsa_public_key_pem: Option<String>,
    jwks_json: Option<String>,
    disabled: bool,
}

/// Standard JWT payload claims parsed during OIDC token validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub exp: usize,
    pub iss: Option<String>,
    pub aud: Option<String>,
}

/// Thread-safe validator for OpenID Connect bearer tokens.
#[derive(Clone)]
pub struct OidcTokenValidator {
    inner: ValidatorInner,
}

#[derive(Clone)]
enum ValidatorInner {
    Active {
        key: DecodingKey,
        validation: Box<Validation>,
    },
    Jwks {
        keys_by_kid: HashMap<String, DecodingKey>,
        fallback_key: Option<DecodingKey>,
        validation: Box<Validation>,
    },
    Disabled,
}

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

#[cfg(test)]
impl OidcConfig {
    /// Creates a default OIDC configuration with optional issuer and audience.
    #[must_use]
    const fn new(issuer: Option<String>, audience: Option<String>) -> Self {
        Self {
            issuer,
            audience,
            hmac_secret: None,
            rsa_public_key_pem: None,
            jwks_json: None,
            disabled: false,
        }
    }

    /// Builds an [`OidcTokenValidator`] according to configured keys and parameters.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] if decoding fails or if configuration lacks keys.
    fn build_validator(&self) -> Result<OidcTokenValidator, AuthError> {
        if self.disabled {
            return Ok(OidcTokenValidator::disabled());
        }

        if let Some(ref jwks) = self.jwks_json {
            return OidcTokenValidator::from_jwks_json(
                jwks,
                self.issuer.clone(),
                self.audience.clone(),
            );
        }

        if let Some(ref pem) = self.rsa_public_key_pem {
            return OidcTokenValidator::from_rsa_pem(
                pem.as_bytes(),
                self.issuer.clone(),
                self.audience.clone(),
            );
        }

        if let Some(ref secret) = self.hmac_secret {
            return Ok(OidcTokenValidator::new_hs256(
                secret.as_bytes(),
                self.issuer.clone(),
                self.audience.clone(),
            ));
        }

        Err(AuthError::InvalidToken(
            "missing OIDC key: provide hmac_secret, rsa_public_key_pem, jwks_json, or enable disabled mode"
                .to_string(),
        ))
    }
}

impl OidcTokenValidator {
    /// Creates a validator using HS256 HMAC shared secret.
    #[must_use]
    pub fn new_hs256(secret: &[u8], issuer: Option<String>, audience: Option<String>) -> Self {
        Self {
            inner: ValidatorInner::Active {
                key: DecodingKey::from_secret(secret),
                validation: Box::new(build_validation(Algorithm::HS256, issuer, audience)),
            },
        }
    }

    /// Creates a validator using an RSA public key in PEM format.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] if the PEM bytes cannot be parsed.
    pub fn from_rsa_pem(
        pem: &[u8],
        issuer: Option<String>,
        audience: Option<String>,
    ) -> Result<Self, AuthError> {
        let key = DecodingKey::from_rsa_pem(pem)
            .map_err(|err| AuthError::InvalidToken(err.to_string()))?;
        Ok(Self {
            inner: ValidatorInner::Active {
                key,
                validation: Box::new(build_validation(Algorithm::RS256, issuer, audience)),
            },
        })
    }

    /// Creates a validator from a JWKS JSON string.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] if the JSON is malformed or contains no valid decoding keys.
    pub fn from_jwks_json(
        jwks_json: &str,
        issuer: Option<String>,
        audience: Option<String>,
    ) -> Result<Self, AuthError> {
        let jwk_set: JwkSet = serde_json::from_str(jwks_json)
            .map_err(|err| AuthError::InvalidToken(err.to_string()))?;
        Self::from_jwk_set(&jwk_set, issuer, audience)
    }

    /// Creates a validator from a parsed JWK set.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] if the JWKS contains no valid decoding keys.
    pub fn from_jwk_set(
        jwk_set: &JwkSet,
        issuer: Option<String>,
        audience: Option<String>,
    ) -> Result<Self, AuthError> {
        let mut keys_by_kid = HashMap::new();
        let mut fallback_key = None;

        for jwk in &jwk_set.keys {
            let Ok(key) = DecodingKey::from_jwk(jwk) else {
                continue;
            };
            if fallback_key.is_none() {
                fallback_key = Some(key.clone());
            }
            if let Some(ref kid) = jwk.common.key_id {
                keys_by_kid.insert(kid.clone(), key);
            }
        }

        let fallback_key = fallback_key.ok_or_else(|| {
            AuthError::InvalidToken("JWKS contains no valid decoding keys".to_string())
        })?;

        Ok(Self {
            inner: ValidatorInner::Jwks {
                keys_by_kid,
                fallback_key: Some(fallback_key),
                validation: Box::new(build_validation(Algorithm::RS256, issuer, audience)),
            },
        })
    }

    /// Creates a validator with signature verification bypassed (for local testing).
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            inner: ValidatorInner::Disabled,
        }
    }

    /// Verifies the signature and claims of an OIDC bearer token.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] if verification fails or the subject claim is invalid.
    pub fn verify_token(&self, token: &str) -> Result<AuthenticatedUser, AuthError> {
        let claims = self.decode_claims(token)?;

        let user_id = UserId::try_new(claims.sub)
            .map_err(|err| AuthError::InvalidToken(format!("invalid subject claim: {err}")))?;

        let email = claims.email.and_then(|e| Email::try_new(e).ok());
        let name = claims.name.and_then(|n| DisplayName::try_new(n).ok());

        Ok(AuthenticatedUser::new(user_id, email, name))
    }

    /// Helper to generate a valid HS256 JWT bearer token for testing.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] if token signing fails.
    pub fn encode_hs256_token(
        secret: &[u8],
        sub: &str,
        email: Option<&str>,
        name: Option<&str>,
        exp: usize,
    ) -> Result<String, AuthError> {
        let claims = JwtClaims {
            sub: sub.to_string(),
            email: email.map(ToString::to_string),
            name: name.map(ToString::to_string),
            exp,
            iss: None,
            aud: None,
        };
        Self::encode_hs256_token_with_claims(secret, &claims)
    }

    /// Helper to generate an HS256 JWT token with explicit claims for testing.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] if token signing fails.
    pub fn encode_hs256_token_with_claims(
        secret: &[u8],
        claims: &JwtClaims,
    ) -> Result<String, AuthError> {
        let header = Header::new(Algorithm::HS256);
        let key = EncodingKey::from_secret(secret);
        encode(&header, claims, &key).map_err(|err| AuthError::InvalidToken(err.to_string()))
    }

    fn decode_claims(&self, token: &str) -> Result<JwtClaims, AuthError> {
        match &self.inner {
            ValidatorInner::Active { key, validation } => {
                let token_data = decode::<JwtClaims>(token, key, validation)
                    .map_err(|err| AuthError::InvalidToken(err.to_string()))?;
                Ok(token_data.claims)
            }
            ValidatorInner::Jwks {
                keys_by_kid,
                fallback_key,
                validation,
            } => decode_jwks_claims(keys_by_kid, fallback_key.as_ref(), validation, token),
            ValidatorInner::Disabled => {
                jsonwebtoken::dangerous::insecure_decode_claims::<JwtClaims>(token)
                    .map_err(|err| AuthError::InvalidToken(err.to_string()))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn build_validation(
    algorithm: Algorithm,
    issuer: Option<String>,
    audience: Option<String>,
) -> Validation {
    let mut validation = Validation::new(algorithm);
    if let Some(ref iss) = issuer {
        validation.set_issuer(&[iss.as_str()]);
    }
    if let Some(ref aud) = audience {
        validation.set_audience(&[aud.as_str()]);
    } else {
        validation.validate_aud = false;
    }
    validation
}

fn decode_jwks_claims(
    keys_by_kid: &HashMap<String, DecodingKey>,
    fallback_key: Option<&DecodingKey>,
    validation: &Validation,
    token: &str,
) -> Result<JwtClaims, AuthError> {
    let header = decode_header(token).map_err(|err| AuthError::InvalidToken(err.to_string()))?;

    let key = match header.kid {
        Some(ref kid) => keys_by_kid.get(kid).ok_or_else(|| {
            AuthError::InvalidToken(format!("no matching JWK found for kid '{kid}'"))
        })?,
        None => fallback_key.ok_or_else(|| {
            AuthError::InvalidToken("token has no kid and no fallback key is available".to_string())
        })?,
    };

    let token_data = decode::<JwtClaims>(token, key, validation)
        .map_err(|err| AuthError::InvalidToken(err.to_string()))?;
    Ok(token_data.claims)
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::jwk::Jwk;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_RSA_PRIVATE_KEY_PEM: &str = r#"-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEA0d5pQ2ovxI4JvNjZV5rWvJkn8MvlYub506299K79okYmRCqJ
boKJ72jk8MFamwMKnwSZ1wK0xCKJc1gwEiraMxeEIV0rl5hNVqk9QkurYrOgpMgC
M9EcXVAvTF1RXVeX4u2gx/nRZeQo7WoWIv8oq4jS5TtIjTjsWCzu5yvsjA5c4TZS
ITjBdS2TLod8xjRzXzORkydj3O4o/JDXQUo0HV28teV//9oIxHajw9EaqzephU9T
MZlCY0yB7zkUw51KBhuh+P0FHWAZNhf6n5b1pbzGcE7e0XyYllYHT/rvCjOptEy+
5KN4/FpSlF+BogQm+mb12NH1Np7ZQsvA8o852QIDAQABAoIBAHk/nOMU6yoMwbV4
NwE09NEbTwcAVbNxLG7OVwuROfnRu3Pwe1v8UQKfQH2Tx+v8stsyRksBRjx2k6Cv
F363gT65XF4GPRQlKrFhr4GoxAiPLMVmxFb5fgCvPdRJqaNa4uZ5UUWXoBHU4oxf
mRB5CB8qOmWzfXQRqgI7W371UoGLEsh6ZoUMuV9UEpW0b0LonZxUtKM1vsRPnVSg
tlnD6SKv631dVx6waDDIFPJ9X7g7qI05z991YIaUi2efoXh+tMXPBus6KpTQlowz
R56G6FmOOkqQmR5nZU3BbHOWv/jbJQSPZka7ZmaNNykrNQkDh/ZEgABxj124HyVM
xhieO/0CgYEA71RgT3FhS8orxxubvwwDHt5OXwSHiM0kSultknXvpXNAkb0NFsrY
0MOte0oolbynbwKXAWSfrMaSK6uLxf+BMPtTqy5FFZqr6gAAFlnKpQU49GFzM8GL
pXu+OIEm6JGB2DY9hjzt6er6dRY0PPTOfhqGVlEpvJT8nd2we7OeTRsCgYEA4Hyz
1SItz6Hliukp19f01zmp6lPd6sY8yJe9DYLrRmq2r9dyMsdvh2KgIgOvtjeRhzXu
LcE6oKBkWQ/3idhADxYL8XLjw1ppoEKi9Sgem0f/+lKPFEs6Ev0faNCa5V1MH6rp
Gkhnv/cfjfAQ5exjrTroSCQ8gnET/0cyCGfgyBsCgYArp2ZR10p5+ox1qvGDrXs8
PCxrZHYcW97M0zsrRdvKf/U9IBCtIZ7epXKVVHeGqHyZ4Q0OMf52E1pCIO5D1VEJ
UC54wN3VDPtimVFqwkIWS5eR69yJijogmyudPuGL9k6K04nxXFfBHR1GEiDBJZ4s
nkX2pKNKRJq3aOJyDddIFQKBgGGfWUUSfAGTUhM6mmu3Ck6eUPoKRrNWShChjCFY
EaCpv5BwjMTwXdvuU9z25W7Ec5DtLwrOwQQhPAJPI0Lau+DToZX6dHn703tR/zPF
oqm8geEGDhqxLCwdz47xSKr+C6Cp3n0Dh7q/aQV8pJyYVDB9MY5N09KWAjA9cdCd
++6JAoGBAOL7XeZzHX/DtuMWtqxS5fV7FcBLMYgSMVGKdB3ocDj+urNQeD7ozBIO
u4pJk9GhZwdujC2SR8ePWQLFR0JmI+5FKYBaYP/k+OHPaxmP535vrdaZdHCF0KhK
Fo8QLEnNhQuL+d9f/Yvwk/gdljds/Z9W3tzVcqs6KDoWwwQAs74w
-----END RSA PRIVATE KEY-----"#;

    fn current_timestamp_secs() -> usize {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs() as usize
    }

    fn generate_test_jwks(kid: Option<&str>) -> (String, EncodingKey) {
        let encoding_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_KEY_PEM.as_bytes())
            .expect("valid test RSA private key");
        let mut jwk = Jwk::from_encoding_key(&encoding_key, Algorithm::RS256)
            .expect("valid JWK from encoding key");
        jwk.common.key_id = kid.map(ToString::to_string);
        let jwk_set = JwkSet { keys: vec![jwk] };
        let jwks_json = serde_json::to_string(&jwk_set).expect("serialize JWKS");
        (jwks_json, encoding_key)
    }

    #[test]
    fn test_valid_token_verification() {
        let secret = b"my-super-secret-key-that-is-long-enough";
        let exp = current_timestamp_secs() + 3600;

        let token = OidcTokenValidator::encode_hs256_token(
            secret,
            "usr_alice",
            Some("alice@example.com"),
            Some("Alice User"),
            exp,
        )
        .expect("token encoding");

        let validator = OidcTokenValidator::new_hs256(secret, None, None);
        let user = validator.verify_token(&token).expect("valid token");

        assert_eq!(user.user_id.as_ref(), "usr_alice");
        assert_eq!(
            user.email.as_ref().map(AsRef::as_ref),
            Some("alice@example.com")
        );
        assert_eq!(user.name.as_ref().map(AsRef::as_ref), Some("Alice User"));
    }

    #[test]
    fn test_expired_token_rejection() {
        let secret = b"my-super-secret-key-that-is-long-enough";
        let exp = current_timestamp_secs() - 3600; // 1 hour in the past

        let token = OidcTokenValidator::encode_hs256_token(secret, "usr_bob", None, None, exp)
            .expect("token encoding");

        let validator = OidcTokenValidator::new_hs256(secret, None, None);
        let result = validator.verify_token(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken(_))));
    }

    #[test]
    fn test_invalid_signature_rejection() {
        let secret_a = b"secret-a-key-which-is-long-enough-12345";
        let secret_b = b"secret-b-key-which-is-different-1234567";
        let exp = current_timestamp_secs() + 3600;

        let token =
            OidcTokenValidator::encode_hs256_token(secret_a, "usr_charlie", None, None, exp)
                .expect("token encoding");

        let validator = OidcTokenValidator::new_hs256(secret_b, None, None);
        let result = validator.verify_token(&token);
        assert!(matches!(result, Err(AuthError::InvalidToken(_))));
    }

    #[test]
    fn test_disabled_validator_bypasses_signature() {
        let secret = b"some-secret-key-not-checked-1234567890";
        let exp = current_timestamp_secs() + 3600;

        let token = OidcTokenValidator::encode_hs256_token(
            secret,
            "usr_dev",
            Some("dev@example.com"),
            None,
            exp,
        )
        .expect("token encoding");

        let validator = OidcTokenValidator::disabled();
        let user = validator
            .verify_token(&token)
            .expect("disabled validator accepts token");
        assert_eq!(user.user_id.as_ref(), "usr_dev");
    }

    #[test]
    fn test_oidc_config_build_validator() {
        let mut config = OidcConfig::new(None, None);
        assert!(config.build_validator().is_err());

        config.disabled = true;
        assert!(config.build_validator().is_ok());

        config.disabled = false;
        config.hmac_secret = Some("secret".into());
        assert!(config.build_validator().is_ok());

        config.hmac_secret = None;
        let (jwks_json, _) = generate_test_jwks(Some("test-kid"));
        config.jwks_json = Some(jwks_json);
        assert!(config.build_validator().is_ok());
    }

    #[test]
    fn test_jwks_from_json_empty_or_malformed_rejected() {
        let empty_result = OidcTokenValidator::from_jwks_json(r#"{"keys":[]}"#, None, None);
        assert!(
            matches!(empty_result, Err(AuthError::InvalidToken(msg)) if msg.contains("no valid decoding keys"))
        );

        let malformed_result = OidcTokenValidator::from_jwks_json("{invalid", None, None);
        assert!(matches!(malformed_result, Err(AuthError::InvalidToken(_))));
    }

    #[test]
    fn test_jwks_valid_signature_and_kid_resolution() {
        let (jwks_json, encoding_key) = generate_test_jwks(Some("dex-key-1"));
        let validator = OidcTokenValidator::from_jwks_json(
            &jwks_json,
            Some("http://localhost:5556/dex".into()),
            Some("luminair-admin-spa".into()),
        )
        .expect("build validator from JWKS");

        let exp = current_timestamp_secs() + 3600;
        let claims = JwtClaims {
            sub: "usr_dex_user".to_string(),
            email: Some("dex@luminair.dev".to_string()),
            name: Some("Dex Admin".to_string()),
            exp,
            iss: Some("http://localhost:5556/dex".to_string()),
            aud: Some("luminair-admin-spa".to_string()),
        };

        // 1. Verify token with matching kid
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("dex-key-1".to_string());
        let token = encode(&header, &claims, &encoding_key).expect("encode RS256 token");
        let user = validator.verify_token(&token).expect("valid RS256 JWKS token");
        assert_eq!(user.user_id.as_ref(), "usr_dex_user");
        assert_eq!(
            user.email.as_ref().map(AsRef::as_ref),
            Some("dex@luminair.dev")
        );

        // 2. Token without kid falls back to primary key
        let header_no_kid = Header::new(Algorithm::RS256);
        let token_no_kid =
            encode(&header_no_kid, &claims, &encoding_key).expect("encode no-kid RS256 token");
        let user_fallback = validator
            .verify_token(&token_no_kid)
            .expect("fallback key verifies");
        assert_eq!(user_fallback.user_id.as_ref(), "usr_dex_user");

        // 3. Token with unknown kid is rejected
        let mut header_unknown_kid = Header::new(Algorithm::RS256);
        header_unknown_kid.kid = Some("non-existent-kid".to_string());
        let token_unknown = encode(&header_unknown_kid, &claims, &encoding_key)
            .expect("encode token with unknown kid");
        let err = validator
            .verify_token(&token_unknown)
            .expect_err("unknown kid rejected");
        assert!(
            matches!(err, AuthError::InvalidToken(msg) if msg.contains("no matching JWK found for kid 'non-existent-kid'"))
        );
    }
}
