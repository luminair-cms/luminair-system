//! Axum request extractors for authenticated callers and onboarding identities.

use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;

use application::context::CallerContext;

use crate::api::error::ApiError;
use crate::api::state::HasAuthResolver;
use crate::auth::claims::AuthenticatedUser;
use crate::auth::error::AuthResolutionError;
use crate::auth::resolver::AuthContextResolver;

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

    async fn from_request_parts(
        parts: &mut Parts,
        state: &St,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = extract_auth_header(parts)?;
        let caller = state
            .auth_resolver()
            .resolve_caller(auth_header)
            .await
            .map_err(map_auth_resolution_error)?;
        Ok(Caller(caller))
    }
}

impl<St> FromRequestParts<St> for OnboardingUser
where
    St: HasAuthResolver,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &St,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = extract_auth_header(parts)?;
        let user = state
            .auth_resolver()
            .resolve_user_identity(auth_header)
            .await
            .map_err(map_auth_resolution_error)?;
        Ok(OnboardingUser(user))
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn extract_auth_header(parts: &Parts) -> Result<Option<&str>, ApiError> {
    match parts.headers.get(AUTHORIZATION) {
        Some(val) => match val.to_str() {
            Ok(s) => Ok(Some(s)),
            Err(_) => Err(ApiError::unauthenticated(
                "unauthenticated",
                "malformed Authorization header: non-ASCII characters",
            )),
        },
        None => Ok(None),
    }
}

fn map_auth_resolution_error(err: AuthResolutionError) -> ApiError {
    match err {
        AuthResolutionError::Auth(auth_err) => {
            ApiError::unauthenticated("unauthenticated", auth_err.to_string())
        }
        AuthResolutionError::Application(app_err) => ApiError::from(app_err),
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;

    use axum::http::Request;
    use domain::auth::user::{ActorId, UserId};
    use domain::schema::SchemaRegistry;
    use domain::system::{LocaleId, SystemConfig, SystemConfigId, SystemContext};

    use crate::api::state::HasSystemContext;
    use crate::auth::error::AuthError;

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
            auth_header: Option<&str>,
        ) -> impl Future<Output = Result<AuthenticatedUser, AuthResolutionError>> + Send {
            let expected = format!("Bearer {}", self.valid_token);
            let matches = auth_header == Some(&expected);
            async move {
                if matches {
                    let user_id = UserId::try_new("usr_test").expect("valid user");
                    Ok(AuthenticatedUser::new(user_id, None, None))
                } else {
                    Err(AuthError::MissingToken.into())
                }
            }
        }

        fn resolve_caller(
            &self,
            auth_header: Option<&str>,
        ) -> impl Future<Output = Result<CallerContext, AuthResolutionError>> + Send {
            let expected = format!("Bearer {}", self.valid_token);
            let matches = auth_header == Some(&expected);
            async move {
                if matches {
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

    #[tokio::test]
    async fn test_caller_extractor_success() {
        let state = fixture_state("secret-123");
        let req = Request::builder()
            .header(AUTHORIZATION, "Bearer secret-123")
            .body(())
            .expect("valid request");
        let (mut parts, _) = req.into_parts();

        let caller = Caller::from_request_parts(&mut parts, &state)
            .await
            .expect("caller should resolve");
        assert_eq!(caller.0.actor, ActorId::User(UserId::try_new("usr_test").unwrap()));
    }

    #[tokio::test]
    async fn test_caller_extractor_missing_token() {
        let state = fixture_state("secret-123");
        let req = Request::builder().body(()).expect("valid request");
        let (mut parts, _) = req.into_parts();

        let err = Caller::from_request_parts(&mut parts, &state)
            .await
            .expect_err("should fail with missing token");
        assert_eq!(err.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_onboarding_user_extractor_success() {
        let state = fixture_state("secret-123");
        let req = Request::builder()
            .header(AUTHORIZATION, "Bearer secret-123")
            .body(())
            .expect("valid request");
        let (mut parts, _) = req.into_parts();

        let user = OnboardingUser::from_request_parts(&mut parts, &state)
            .await
            .expect("user identity should resolve");
        assert_eq!(user.0.user_id.as_ref(), "usr_test");
    }

    #[tokio::test]
    async fn test_onboarding_user_extractor_missing_token() {
        let state = fixture_state("secret-123");
        let req = Request::builder().body(()).expect("valid request");
        let (mut parts, _) = req.into_parts();

        let err = OnboardingUser::from_request_parts(&mut parts, &state)
            .await
            .expect_err("should fail with missing token");
        assert_eq!(err.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    }
}
