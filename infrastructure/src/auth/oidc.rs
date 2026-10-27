//! OIDC JWT verification and token decoding.

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use domain::auth::user::UserId;
use domain::common::{DisplayName, Email};

use crate::auth::claims::AuthenticatedUser;
use crate::auth::error::AuthError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Configuration parameters for OpenID Connect token validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcConfig {
    pub issuer: Option<String>,
    pub audience: Option<String>,
    pub hmac_secret: Option<String>,
    pub rsa_public_key_pem: Option<String>,
    pub disabled: bool,
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
    Disabled,
}

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

impl OidcConfig {
    /// Creates a default OIDC configuration with optional issuer and audience.
    #[must_use]
    pub const fn new(issuer: Option<String>, audience: Option<String>) -> Self {
        Self {
            issuer,
            audience,
            hmac_secret: None,
            rsa_public_key_pem: None,
            disabled: false,
        }
    }

    /// Builds an [`OidcTokenValidator`] according to configured keys and parameters.
    ///
    /// # Errors
    /// Returns [`AuthError::InvalidToken`] if RSA PEM decoding fails or if configuration lacks keys.
    pub fn build_validator(&self) -> Result<OidcTokenValidator, AuthError> {
        if self.disabled {
            return Ok(OidcTokenValidator::disabled());
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
            "missing OIDC key: provide hmac_secret, rsa_public_key_pem, or enable disabled mode"
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

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn current_timestamp_secs() -> usize {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs() as usize
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
        assert_eq!(
            user.name.as_ref().map(AsRef::as_ref),
            Some("Alice User")
        );
    }

    #[test]
    fn test_expired_token_rejection() {
        let secret = b"my-super-secret-key-that-is-long-enough";
        let exp = current_timestamp_secs() - 3600; // 1 hour in the past

        let token = OidcTokenValidator::encode_hs256_token(
            secret,
            "usr_bob",
            None,
            None,
            exp,
        )
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

        let token = OidcTokenValidator::encode_hs256_token(
            secret_a,
            "usr_charlie",
            None,
            None,
            exp,
        )
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
        let user = validator.verify_token(&token).expect("disabled validator accepts token");
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
    }
}
