//! Authentication and authorization error types.

pub use luminair_http::auth::{AuthError, AuthResolutionError};

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use application::errors::ApplicationError;

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
            AuthError::InvalidToken("expired signature".into()).to_string(),
            "invalid or expired authentication token: expired signature"
        );
    }

    #[test]
    fn test_auth_resolution_error_from_auth_error() {
        let err: AuthResolutionError = AuthError::MissingToken.into();
        assert_eq!(err.to_string(), "missing Authorization header");
        assert!(matches!(
            err,
            AuthResolutionError::Auth(AuthError::MissingToken)
        ));
    }

    #[test]
    fn test_auth_resolution_error_from_application_error() {
        let app_err = ApplicationError::Internal("db timeout".into());
        let res_err: AuthResolutionError = app_err.into();
        assert_eq!(res_err.to_string(), "internal error: db timeout");
        assert!(matches!(res_err, AuthResolutionError::Application(_)));
    }
}
