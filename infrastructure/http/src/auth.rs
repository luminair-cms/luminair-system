//! Authentication credentials, models, errors, and context resolution trait.

use std::future::Future;

use application::context::CallerContext;
use application::errors::ApplicationError;
use domain::auth::user::UserId;
use domain::common::{DisplayName, Email};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Credentials & Identity Models
// ---------------------------------------------------------------------------

/// Transport-agnostic authentication credentials extracted from incoming requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthCredentials<'a> {
    /// In-process token validation (Bearer JWT or Service Token).
    BearerToken(&'a str),
    /// Verified edge claims injected by API Gateway.
    EdgeCognito {
        user_id: &'a str,
        email: Option<&'a str>,
        name: Option<&'a str>,
    },
}

/// Verified human identity extracted from an authentication token or edge claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    pub user_id: UserId,
    pub email: Option<Email>,
    pub name: Option<DisplayName>,
}

impl AuthenticatedUser {
    /// Creates a new authenticated user identity.
    #[must_use]
    pub const fn new(user_id: UserId, email: Option<Email>, name: Option<DisplayName>) -> Self {
        Self {
            user_id,
            email,
            name,
        }
    }
}

// ---------------------------------------------------------------------------
// Error Types
// ---------------------------------------------------------------------------

/// Errors occurring during credential extraction and signature/claims verification.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    #[error("missing Authorization header")]
    MissingToken,
    #[error("malformed Authorization header: expected 'Bearer <token>'")]
    MalformedHeader,
    #[error("invalid or expired authentication token: {0}")]
    InvalidToken(String),
}

/// Errors occurring during caller context and onboarding resolution.
#[derive(Debug, Error)]
pub enum AuthResolutionError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Application(#[from] ApplicationError),
}

// ---------------------------------------------------------------------------
// Port Trait
// ---------------------------------------------------------------------------

/// Port trait defining operations to resolve incoming credentials into caller identities.
pub trait AuthContextResolver: Send + Sync + 'static {
    /// Resolves and verifies the human user identity from the supplied credentials.
    fn resolve_user_identity(
        &self,
        credentials: AuthCredentials<'_>,
    ) -> impl Future<Output = Result<AuthenticatedUser, AuthResolutionError>> + Send;

    /// Resolves the authenticated caller and their roles, checking onboarding status.
    fn resolve_caller(
        &self,
        credentials: AuthCredentials<'_>,
    ) -> impl Future<Output = Result<CallerContext, AuthResolutionError>> + Send;
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_display() {
        assert_eq!(
            AuthError::MissingToken.to_string(),
            "missing Authorization header"
        );
        assert_eq!(
            AuthError::MalformedHeader.to_string(),
            "malformed Authorization header: expected 'Bearer <token>'"
        );
        assert_eq!(
            AuthError::InvalidToken("expired".into()).to_string(),
            "invalid or expired authentication token: expired"
        );
    }

    #[test]
    fn test_authenticated_user_new() {
        let user_id = UserId::try_new("usr_123").expect("valid user id");
        let user = AuthenticatedUser::new(user_id.clone(), None, None);
        assert_eq!(user.user_id, user_id);
        assert_eq!(user.email, None);
        assert_eq!(user.name, None);
    }
}
