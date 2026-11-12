//! Axum request extractors for authenticated callers and onboarding identities.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;

use application::context::CallerContext;

use crate::auth::{
    AuthContextResolver, AuthCredentials, AuthError, AuthResolutionError, AuthenticatedUser,
};
use crate::error::ApiError;
use crate::state::HasAuthResolver;

// ---------------------------------------------------------------------------
// Header Constants
// ---------------------------------------------------------------------------

pub const HEADER_COGNITO_SUB: &str = "x-cognito-sub";
pub const HEADER_COGNITO_EMAIL: &str = "x-cognito-email";
pub const HEADER_COGNITO_NAME: &str = "x-cognito-name";

// ---------------------------------------------------------------------------
// Extractor Types
// ---------------------------------------------------------------------------

/// Axum extractor for resolving the authenticated caller context and assigned RBAC roles.
///
/// Handlers requesting `Caller` are protected endpoints requiring valid credentials
/// and active role assignments. Unassigned users fail with 403 onboarding errors.
#[derive(Debug, Clone)]
pub struct Caller(pub CallerContext);

/// Axum extractor for resolving the authenticated user identity during initial onboarding.
///
/// Handlers requesting `OnboardingUser` permit authenticated IdP users who do not
/// yet have assigned system roles (e.g. submitting an access request).
#[derive(Debug, Clone)]
pub struct OnboardingUser(pub AuthenticatedUser);

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl<St> FromRequestParts<St> for Caller
where
    St: HasAuthResolver,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &St) -> Result<Self, Self::Rejection> {
        let credentials = extract_credentials(parts)?;
        let caller = state
            .auth_resolver()
            .resolve_caller(credentials)
            .await
            .map_err(ApiError::from)?;
        Ok(Caller(caller))
    }
}

impl<St> FromRequestParts<St> for OnboardingUser
where
    St: HasAuthResolver,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &St) -> Result<Self, Self::Rejection> {
        let credentials = extract_credentials(parts)?;
        let user = state
            .auth_resolver()
            .resolve_user_identity(credentials)
            .await
            .map_err(ApiError::from)?;
        Ok(OnboardingUser(user))
    }
}

// ---------------------------------------------------------------------------
// Extraction Helpers
// ---------------------------------------------------------------------------

/// Extracts [`AuthCredentials`] by inspecting trusted edge headers (`x-cognito-sub`)
/// before falling back to standard `Authorization: Bearer <token>` headers.
///
/// # Errors
/// Returns [`AuthError::MissingToken`] if neither edge claims nor bearer tokens are present,
/// or [`AuthError::MalformedHeader`] if headers contain invalid characters or incorrect format.
pub fn extract_credentials<'a>(parts: &'a Parts) -> Result<AuthCredentials<'a>, AuthError> {
    if let Some(sub_header) = parts.headers.get(HEADER_COGNITO_SUB) {
        let user_id = sub_header
            .to_str()
            .map_err(|_| AuthError::MalformedHeader)?;
        let trimmed_user = user_id.trim();
        if !trimmed_user.is_empty() {
            let email = parts
                .headers
                .get(HEADER_COGNITO_EMAIL)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let name = parts
                .headers
                .get(HEADER_COGNITO_NAME)
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty());

            return Ok(AuthCredentials::EdgeCognito {
                user_id: trimmed_user,
                email,
                name,
            });
        }
    }

    if let Some(auth_val) = parts.headers.get(AUTHORIZATION) {
        let header = auth_val.to_str().map_err(|_| AuthError::MalformedHeader)?;
        let trimmed = header.trim();
        if trimmed.is_empty() {
            return Err(AuthError::MissingToken);
        }
        if !trimmed.starts_with("Bearer ") {
            return Err(AuthError::MalformedHeader);
        }
        let token = trimmed["Bearer ".len()..].trim();
        if token.is_empty() {
            return Err(AuthError::MalformedHeader);
        }
        return Ok(AuthCredentials::BearerToken(token));
    }

    Err(AuthError::MissingToken)
}

/// Maps an [`AuthResolutionError`] into an [`ApiError`].
#[must_use]
pub fn map_auth_resolution_error(err: AuthResolutionError) -> ApiError {
    ApiError::from(err)
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;

    use axum::http::HeaderValue;
    use axum::http::Request;
    use domain::auth::user::{ActorId, UserId};
    use domain::common::{DisplayName, Email};
    use domain::schema::SchemaRegistry;
    use domain::system::{LocaleId, SystemConfig, SystemConfigId, SystemContext};

    use crate::auth::AuthError;
    use crate::state::HasSystemContext;

    #[derive(Clone)]
    struct MockState {
        context: &'static SystemContext,
        resolver: Arc<MockAuthResolver>,
    }

    impl HasSystemContext for MockState {
        fn system_context(&self) -> &'static SystemContext {
            self.context
        }
    }

    impl HasAuthResolver for MockState {
        type Auth = MockAuthResolver;

        fn auth_resolver(&self) -> &Self::Auth {
            &self.resolver
        }
    }

    struct MockAuthResolver {
        valid_token: String,
    }

    impl AuthContextResolver for MockAuthResolver {
        fn resolve_user_identity(
            &self,
            credentials: AuthCredentials<'_>,
        ) -> impl Future<Output = Result<AuthenticatedUser, AuthResolutionError>> + Send {
            let is_valid = match &credentials {
                AuthCredentials::BearerToken(t) => *t == self.valid_token,
                AuthCredentials::EdgeCognito { user_id, .. } => *user_id == "usr_cognito",
            };
            async move {
                if is_valid {
                    let user_id = UserId::try_new("usr_test").expect("valid user");
                    let email = Email::try_new("test@example.com").ok();
                    let name = DisplayName::try_new("Test User").ok();
                    Ok(AuthenticatedUser::new(user_id, email, name))
                } else {
                    Err(AuthError::MissingToken.into())
                }
            }
        }

        fn resolve_caller(
            &self,
            credentials: AuthCredentials<'_>,
        ) -> impl Future<Output = Result<CallerContext, AuthResolutionError>> + Send {
            let is_valid = match &credentials {
                AuthCredentials::BearerToken(t) => *t == self.valid_token,
                AuthCredentials::EdgeCognito { user_id, .. } => *user_id == "usr_cognito",
            };
            async move {
                if is_valid {
                    let user_id = UserId::try_new("usr_test").expect("valid user");
                    Ok(CallerContext::new(ActorId::User(user_id), Vec::new()))
                } else {
                    Err(AuthError::MissingToken.into())
                }
            }
        }
    }

    fn fixture_state(token: &str) -> MockState {
        let registry = SchemaRegistry::new(vec![]);
        let locale = LocaleId::try_new("en").expect("valid locale");
        let config = SystemConfig::new(
            SystemConfigId::new(uuid::Uuid::now_v7()),
            vec![locale.clone()],
            locale,
        )
        .expect("valid config");
        let context = Box::leak(Box::new(SystemContext::new(registry, config)));
        MockState {
            context,
            resolver: Arc::new(MockAuthResolver {
                valid_token: token.to_string(),
            }),
        }
    }

    // -----------------------------------------------------------------------
    // Extractor Integration Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_caller_extractor_bearer_token_success() {
        let state = fixture_state("secret-123");
        let req = Request::builder()
            .header(AUTHORIZATION, "Bearer secret-123")
            .body(())
            .expect("valid request");
        let (mut parts, _) = req.into_parts();

        let caller = Caller::from_request_parts(&mut parts, &state)
            .await
            .expect("caller should resolve");
        assert_eq!(
            caller.0.actor,
            ActorId::User(UserId::try_new("usr_test").unwrap())
        );
    }

    #[tokio::test]
    async fn test_caller_extractor_edge_cognito_success() {
        let state = fixture_state("secret-123");
        let req = Request::builder()
            .header(HEADER_COGNITO_SUB, "usr_cognito")
            .header(HEADER_COGNITO_EMAIL, "test@example.com")
            .header(HEADER_COGNITO_NAME, "Cognito User")
            .body(())
            .expect("valid request");
        let (mut parts, _) = req.into_parts();

        let caller = Caller::from_request_parts(&mut parts, &state)
            .await
            .expect("caller should resolve");
        assert_eq!(
            caller.0.actor,
            ActorId::User(UserId::try_new("usr_test").unwrap())
        );
    }

    #[tokio::test]
    async fn test_caller_extractor_missing_credentials() {
        let state = fixture_state("secret-123");
        let req = Request::builder().body(()).expect("valid request");
        let (mut parts, _) = req.into_parts();

        let err = Caller::from_request_parts(&mut parts, &state)
            .await
            .expect_err("should fail with missing credentials");
        assert_eq!(err.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_onboarding_user_extractor_edge_cognito_success() {
        let state = fixture_state("secret-123");
        let req = Request::builder()
            .header(HEADER_COGNITO_SUB, "usr_cognito")
            .header(HEADER_COGNITO_EMAIL, "test@example.com")
            .header(HEADER_COGNITO_NAME, "Cognito User")
            .body(())
            .expect("valid request");
        let (mut parts, _) = req.into_parts();

        let user = OnboardingUser::from_request_parts(&mut parts, &state)
            .await
            .expect("onboarding user should resolve");
        assert_eq!(user.0.user_id.as_ref(), "usr_test");
        assert_eq!(user.0.email.unwrap().as_ref(), "test@example.com");
        assert_eq!(user.0.name.unwrap().as_ref(), "Test User");
    }

    #[tokio::test]
    async fn test_onboarding_user_extractor_bearer_token_success() {
        let state = fixture_state("secret-123");
        let req = Request::builder()
            .header(AUTHORIZATION, "Bearer secret-123")
            .body(())
            .expect("valid request");
        let (mut parts, _) = req.into_parts();

        let user = OnboardingUser::from_request_parts(&mut parts, &state)
            .await
            .expect("onboarding user should resolve");
        assert_eq!(user.0.user_id.as_ref(), "usr_test");
    }

    #[tokio::test]
    async fn test_onboarding_user_extractor_missing_credentials() {
        let state = fixture_state("secret-123");
        let req = Request::builder().body(()).expect("valid request");
        let (mut parts, _) = req.into_parts();

        let err = OnboardingUser::from_request_parts(&mut parts, &state)
            .await
            .expect_err("should fail with missing credentials");
        assert_eq!(err.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    }

    // -----------------------------------------------------------------------
    // Direct `extract_credentials` Unit Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_credentials_edge_cognito_full() {
        let req = Request::builder()
            .header(HEADER_COGNITO_SUB, "usr_edge_123")
            .header(HEADER_COGNITO_EMAIL, "edge@example.com")
            .header(HEADER_COGNITO_NAME, "Edge User")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let creds = extract_credentials(&parts).expect("should extract edge credentials");
        assert_eq!(
            creds,
            AuthCredentials::EdgeCognito {
                user_id: "usr_edge_123",
                email: Some("edge@example.com"),
                name: Some("Edge User"),
            }
        );
    }

    #[test]
    fn test_extract_credentials_edge_cognito_minimal() {
        let req = Request::builder()
            .header(HEADER_COGNITO_SUB, "usr_edge_minimal")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let creds = extract_credentials(&parts).expect("should extract minimal edge credentials");
        assert_eq!(
            creds,
            AuthCredentials::EdgeCognito {
                user_id: "usr_edge_minimal",
                email: None,
                name: None,
            }
        );
    }

    #[test]
    fn test_extract_credentials_edge_cognito_empty_email_and_name_filtered() {
        let req = Request::builder()
            .header(HEADER_COGNITO_SUB, "usr_edge_filtered")
            .header(HEADER_COGNITO_EMAIL, "   ")
            .header(HEADER_COGNITO_NAME, "")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let creds = extract_credentials(&parts).expect("should extract edge credentials");
        assert_eq!(
            creds,
            AuthCredentials::EdgeCognito {
                user_id: "usr_edge_filtered",
                email: None,
                name: None,
            }
        );
    }

    #[test]
    fn test_extract_credentials_bearer_token() {
        let req = Request::builder()
            .header(AUTHORIZATION, "Bearer my-jwt-token-xyz")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let creds = extract_credentials(&parts).expect("should extract bearer token");
        assert_eq!(creds, AuthCredentials::BearerToken("my-jwt-token-xyz"));
    }

    #[test]
    fn test_extract_credentials_precedence_edge_over_bearer() {
        let req = Request::builder()
            .header(HEADER_COGNITO_SUB, "usr_edge_priority")
            .header(AUTHORIZATION, "Bearer ignored-token")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let creds = extract_credentials(&parts).expect("should extract edge credentials");
        assert_eq!(
            creds,
            AuthCredentials::EdgeCognito {
                user_id: "usr_edge_priority",
                email: None,
                name: None,
            }
        );
    }

    #[test]
    fn test_extract_credentials_empty_edge_falls_back_to_bearer() {
        let req = Request::builder()
            .header(HEADER_COGNITO_SUB, "   ")
            .header(AUTHORIZATION, "Bearer fallback-token")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let creds = extract_credentials(&parts).expect("should fall back to bearer token");
        assert_eq!(creds, AuthCredentials::BearerToken("fallback-token"));
    }

    #[test]
    fn test_extract_credentials_missing_all() {
        let req = Request::builder().body(()).unwrap();
        let (parts, _) = req.into_parts();

        let err = extract_credentials(&parts).expect_err("should fail when no headers present");
        assert_eq!(err, AuthError::MissingToken);
    }

    #[test]
    fn test_extract_credentials_malformed_bearer_scheme() {
        let req = Request::builder()
            .header(AUTHORIZATION, "Basic dXNlcjpwYXNz")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let err = extract_credentials(&parts).expect_err("should reject Basic auth");
        assert_eq!(err, AuthError::MalformedHeader);
    }

    #[test]
    fn test_extract_credentials_empty_bearer_token() {
        let req = Request::builder()
            .header(AUTHORIZATION, "Bearer ")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let err = extract_credentials(&parts).expect_err("should reject empty bearer token");
        assert_eq!(err, AuthError::MalformedHeader);

        let req2 = Request::builder()
            .header(AUTHORIZATION, "Bearer    ")
            .body(())
            .unwrap();
        let (parts2, _) = req2.into_parts();

        let err2 = extract_credentials(&parts2).expect_err("should reject whitespace bearer token");
        assert_eq!(err2, AuthError::MalformedHeader);
    }

    #[test]
    fn test_extract_credentials_empty_authorization_header() {
        let req = Request::builder()
            .header(AUTHORIZATION, "")
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();

        let err =
            extract_credentials(&parts).expect_err("should fail with MissingToken for empty auth");
        assert_eq!(err, AuthError::MissingToken);
    }

    #[test]
    fn test_extract_credentials_non_ascii_sub() {
        let mut req = Request::builder().body(()).unwrap();
        let mut val = HeaderValue::from_bytes(b"\xFF\xFE\xFD").unwrap();
        val.set_sensitive(true);
        req.headers_mut().insert(HEADER_COGNITO_SUB, val);
        let (parts, _) = req.into_parts();

        let err = extract_credentials(&parts).expect_err("should reject non-ASCII sub");
        assert_eq!(err, AuthError::MalformedHeader);
    }

    #[test]
    fn test_extract_credentials_non_ascii_authorization() {
        let mut req = Request::builder().body(()).unwrap();
        let mut val = HeaderValue::from_bytes(b"\xFF\xFE\xFD").unwrap();
        val.set_sensitive(true);
        req.headers_mut().insert(AUTHORIZATION, val);
        let (parts, _) = req.into_parts();

        let err = extract_credentials(&parts).expect_err("should reject non-ASCII authorization");
        assert_eq!(err, AuthError::MalformedHeader);
    }
}
