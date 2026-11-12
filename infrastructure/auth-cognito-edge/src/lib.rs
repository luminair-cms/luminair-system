//! Edge-authenticated AWS Cognito authorization context resolver.
//!
//! Operates with zero local RSA cryptography and zero outbound network calls,
//! resolving verified identity claims injected by Amazon API Gateway HTTP API (v2) parameter mappings.
//! Adheres strictly to ADR-006: strictly read-only, performing zero writes during authentication.

use application::context::CallerContext;
use application::errors::ApplicationError;
use domain::auth::access_request::AccessRequestStatus;
use domain::auth::ports::{AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository};
use domain::auth::role::{RoleId, UserRoleAssignment};
use domain::auth::user::{ActorId, UserId};
use domain::common::{DisplayName, Email};
use luminair_http::auth::{
    AuthContextResolver, AuthCredentials, AuthError, AuthResolutionError, AuthenticatedUser,
};
use luminair_persistence::repositories::{
    SqlxAccessRequestRepository, SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Edge-authenticated caller and onboarding identity resolver.
///
/// Designed for AWS Lambda deployments fronted by Amazon API Gateway HTTP API (v2).
/// Relies entirely on verified edge header claims (`x-cognito-sub`, `x-cognito-email`, `x-cognito-name`).
#[derive(Debug, Clone)]
pub struct CognitoEdgeAuthResolver<
    A = SqlxAccessRequestRepository,
    U = SqlxUserRoleAssignmentRepository,
    R = SqlxRoleRepository,
> {
    access_request_repo: A,
    assignment_repo: U,
    role_repo: R,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl
    CognitoEdgeAuthResolver<
        SqlxAccessRequestRepository,
        SqlxUserRoleAssignmentRepository,
        SqlxRoleRepository,
    >
{
    /// Creates a new `CognitoEdgeAuthResolver` with SQLx repositories bound to the given connection pool.
    #[must_use]
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            access_request_repo: SqlxAccessRequestRepository::new(pool.clone()),
            assignment_repo: SqlxUserRoleAssignmentRepository::new(pool.clone()),
            role_repo: SqlxRoleRepository::new(pool),
        }
    }
}

impl<A, U, R> CognitoEdgeAuthResolver<A, U, R> {
    /// Creates a new `CognitoEdgeAuthResolver` with explicit repository implementations.
    pub fn with_repositories(access_request_repo: A, assignment_repo: U, role_repo: R) -> Self {
        Self {
            access_request_repo,
            assignment_repo,
            role_repo,
        }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl<A, U, R> AuthContextResolver for CognitoEdgeAuthResolver<A, U, R>
where
    A: AccessRequestRepository + 'static,
    U: UserRoleAssignmentRepository + 'static,
    R: RoleRepository + 'static,
{
    async fn resolve_user_identity(
        &self,
        credentials: AuthCredentials<'_>,
    ) -> Result<AuthenticatedUser, AuthResolutionError> {
        match credentials {
            AuthCredentials::EdgeCognito {
                user_id,
                email,
                name,
            } => {
                let trimmed = user_id.trim();
                if trimmed.is_empty() {
                    return Err(AuthError::MissingToken.into());
                }
                let user_id = UserId::try_new(trimmed)
                    .map_err(|e| AuthError::InvalidToken(format!("invalid edge user ID: {e}")))?;
                let email = email.map(Email::try_new).transpose().map_err(|e| {
                    AuthError::InvalidToken(format!("invalid edge email claim: {e}"))
                })?;
                let name = name.map(DisplayName::try_new).transpose().map_err(|e| {
                    AuthError::InvalidToken(format!("invalid edge name claim: {e}"))
                })?;

                Ok(AuthenticatedUser::new(user_id, email, name))
            }
            AuthCredentials::BearerToken(_) => Err(AuthError::MissingToken.into()),
        }
    }

    async fn resolve_caller(
        &self,
        credentials: AuthCredentials<'_>,
    ) -> Result<CallerContext, AuthResolutionError> {
        let user = self.resolve_user_identity(credentials).await?;

        let assignments = self
            .assignment_repo
            .find_by_user(&user.user_id)
            .await
            .map_err(ApplicationError::Domain)?;

        if assignments.is_empty() {
            return Err(self.resolve_onboarding_error(user.user_id).await);
        }

        self.resolve_user_roles(user.user_id, assignments).await
    }
}

impl<A, U, R> CognitoEdgeAuthResolver<A, U, R>
where
    A: AccessRequestRepository,
    R: RoleRepository,
{
    async fn resolve_onboarding_error(&self, user_id: UserId) -> AuthResolutionError {
        let maybe_req = self
            .access_request_repo
            .find_by_user(&user_id)
            .await
            .map_err(ApplicationError::Domain);

        let req = match maybe_req {
            Ok(Some(r)) => r,
            Ok(None) => return ApplicationError::access_not_requested(user_id).into(),
            Err(err) => return err.into(),
        };

        match req.status {
            AccessRequestStatus::Pending => {
                ApplicationError::access_pending(user_id, req.requested_at).into()
            }
            AccessRequestStatus::Rejected { reason } => {
                ApplicationError::access_rejected(user_id, reason).into()
            }
            AccessRequestStatus::Approved => ApplicationError::access_not_requested(user_id).into(),
        }
    }

    async fn resolve_user_roles(
        &self,
        user_id: UserId,
        assignments: Vec<UserRoleAssignment>,
    ) -> Result<CallerContext, AuthResolutionError> {
        let role_ids: Vec<RoleId> = assignments.into_iter().map(|a| a.role_id).collect();
        let roles = self
            .role_repo
            .find_by_ids(&role_ids)
            .await
            .map_err(ApplicationError::Domain)?;
        Ok(CallerContext::new(ActorId::User(user_id), roles))
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::auth::access_request::{AccessRequest, AccessRequestId};
    use domain::auth::role::{Role, UserRoleAssignmentId};
    use domain::errors::DomainError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    #[derive(Default)]
    struct MockAccessRequestRepo {
        request: Option<AccessRequest>,
        write_call_count: AtomicUsize,
    }

    impl AccessRequestRepository for MockAccessRequestRepo {
        async fn find_by_id(
            &self,
            _id: AccessRequestId,
        ) -> Result<Option<AccessRequest>, DomainError> {
            Ok(self.request.clone())
        }

        async fn find_by_user(
            &self,
            _user_id: &UserId,
        ) -> Result<Option<AccessRequest>, DomainError> {
            Ok(self.request.clone())
        }

        async fn find_active_by_user(
            &self,
            _user_id: &UserId,
        ) -> Result<Option<AccessRequest>, DomainError> {
            Ok(self.request.clone())
        }

        async fn find_pending(&self) -> Result<Vec<AccessRequest>, DomainError> {
            Ok(Vec::new())
        }

        async fn save(&self, _request: &AccessRequest) -> Result<(), DomainError> {
            self.write_call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockAssignmentRepo {
        assignments: Vec<UserRoleAssignment>,
        write_call_count: AtomicUsize,
    }

    impl UserRoleAssignmentRepository for MockAssignmentRepo {
        async fn find_by_user(
            &self,
            _user_id: &UserId,
        ) -> Result<Vec<UserRoleAssignment>, DomainError> {
            Ok(self.assignments.clone())
        }

        async fn exists_admin(&self, _admin_role_id: RoleId) -> Result<bool, DomainError> {
            Ok(false)
        }

        async fn save(&self, _assignment: &UserRoleAssignment) -> Result<(), DomainError> {
            self.write_call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn save_all(&self, _assignments: &[UserRoleAssignment]) -> Result<(), DomainError> {
            self.write_call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn delete(&self, _id: UserRoleAssignmentId) -> Result<(), DomainError> {
            self.write_call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockRoleRepo {
        roles: Vec<Role>,
        write_call_count: AtomicUsize,
    }

    impl RoleRepository for MockRoleRepo {
        async fn find_by_id(&self, _id: RoleId) -> Result<Option<Role>, DomainError> {
            Ok(None)
        }

        async fn find_by_name(&self, _name: &str) -> Result<Option<Role>, DomainError> {
            Ok(None)
        }

        async fn find_by_ids(&self, _ids: &[RoleId]) -> Result<Vec<Role>, DomainError> {
            Ok(self.roles.clone())
        }

        async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
            Ok(self.roles.clone())
        }

        async fn save(&self, _role: &Role) -> Result<(), DomainError> {
            self.write_call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn test_role(name: &str) -> Role {
        Role {
            id: RoleId::new(Uuid::now_v7()),
            name: name.to_string(),
            description: None,
            permissions: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // resolve_user_identity Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_user_identity_edge_cognito() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_alice",
            email: Some("alice@example.com"),
            name: Some("Alice Smith"),
        };

        let user = resolver
            .resolve_user_identity(credentials)
            .await
            .expect("should resolve identity");

        assert_eq!(user.user_id.as_ref(), "usr_alice");
        assert_eq!(user.email.unwrap().as_ref(), "alice@example.com");
        assert_eq!(user.name.unwrap().as_ref(), "Alice Smith");
    }

    #[tokio::test]
    async fn test_resolve_user_identity_edge_cognito_minimal() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_alice_minimal",
            email: None,
            name: None,
        };

        let user = resolver
            .resolve_user_identity(credentials)
            .await
            .expect("should resolve identity");

        assert_eq!(user.user_id.as_ref(), "usr_alice_minimal");
        assert_eq!(user.email, None);
        assert_eq!(user.name, None);
    }

    #[tokio::test]
    async fn test_resolve_user_identity_rejects_bearer_token() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::BearerToken("jwt-token");
        let err = resolver
            .resolve_user_identity(credentials)
            .await
            .expect_err("should reject bearer token");

        assert!(matches!(
            err,
            AuthResolutionError::Auth(AuthError::MissingToken)
        ));
    }

    #[tokio::test]
    async fn test_resolve_user_identity_rejects_empty_user_id() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let empty_credentials = AuthCredentials::EdgeCognito {
            user_id: "",
            email: None,
            name: None,
        };
        let err = resolver
            .resolve_user_identity(empty_credentials)
            .await
            .expect_err("should reject empty user_id");
        assert!(matches!(
            err,
            AuthResolutionError::Auth(AuthError::MissingToken)
        ));

        let whitespace_credentials = AuthCredentials::EdgeCognito {
            user_id: "    ",
            email: None,
            name: None,
        };
        let err2 = resolver
            .resolve_user_identity(whitespace_credentials)
            .await
            .expect_err("should reject whitespace user_id");
        assert!(matches!(
            err2,
            AuthResolutionError::Auth(AuthError::MissingToken)
        ));
    }

    #[tokio::test]
    async fn test_resolve_user_identity_rejects_invalid_user_id() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        // UserId maximum length is 255 characters
        let too_long_id = "a".repeat(300);
        let credentials = AuthCredentials::EdgeCognito {
            user_id: &too_long_id,
            email: None,
            name: None,
        };

        let err = resolver
            .resolve_user_identity(credentials)
            .await
            .expect_err("should reject too long user_id");
        assert!(matches!(
            err,
            AuthResolutionError::Auth(AuthError::InvalidToken(_))
        ));
    }

    #[tokio::test]
    async fn test_resolve_user_identity_rejects_invalid_email() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_valid",
            email: Some("not-an-email"),
            name: None,
        };

        let err = resolver
            .resolve_user_identity(credentials)
            .await
            .expect_err("should reject invalid email");
        assert!(matches!(
            err,
            AuthResolutionError::Auth(AuthError::InvalidToken(_))
        ));
    }

    #[tokio::test]
    async fn test_resolve_user_identity_rejects_invalid_name() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        // DisplayName maximum length is 255 characters
        let too_long_name = "x".repeat(300);
        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_valid",
            email: None,
            name: Some(&too_long_name),
        };

        let err = resolver
            .resolve_user_identity(credentials)
            .await
            .expect_err("should reject too long name");
        assert!(matches!(
            err,
            AuthResolutionError::Auth(AuthError::InvalidToken(_))
        ));
    }

    // -----------------------------------------------------------------------
    // resolve_caller Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_resolve_caller_with_roles() {
        let role = test_role("admin");
        let role_id = role.id;
        let user_id = UserId::try_new("usr_bob").unwrap();

        let access_repo = MockAccessRequestRepo::default();
        let assignment_repo = MockAssignmentRepo {
            assignments: vec![UserRoleAssignment {
                id: UserRoleAssignmentId::new(Uuid::now_v7()),
                user_id: user_id.clone(),
                role_id,
                granted_at: Utc::now(),
                granted_by: None,
            }],
            write_call_count: AtomicUsize::new(0),
        };
        let role_repo = MockRoleRepo {
            roles: vec![role],
            write_call_count: AtomicUsize::new(0),
        };

        let resolver =
            CognitoEdgeAuthResolver::with_repositories(access_repo, assignment_repo, role_repo);

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_bob",
            email: None,
            name: None,
        };

        let caller = resolver
            .resolve_caller(credentials)
            .await
            .expect("should resolve caller");

        assert_eq!(caller.actor, ActorId::User(user_id));
        assert_eq!(caller.roles.len(), 1);

        // ADR-006: Strictly read-only verification
        assert_eq!(
            resolver
                .access_request_repo
                .write_call_count
                .load(Ordering::SeqCst),
            0
        );
        assert_eq!(
            resolver
                .assignment_repo
                .write_call_count
                .load(Ordering::SeqCst),
            0
        );
        assert_eq!(
            resolver.role_repo.write_call_count.load(Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn test_resolve_caller_onboarding_pending() {
        let user_id = UserId::try_new("usr_charlie").unwrap();
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo {
                request: Some(AccessRequest {
                    id: AccessRequestId::new(Uuid::now_v7()),
                    user_id: user_id.clone(),
                    email: None,
                    name: None,
                    requested_at: Utc::now(),
                    status: AccessRequestStatus::Pending,
                    reviewed_by: None,
                    reviewed_at: None,
                    assigned_roles: Vec::new(),
                }),
                write_call_count: AtomicUsize::new(0),
            },
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_charlie",
            email: None,
            name: None,
        };

        let err = resolver
            .resolve_caller(credentials)
            .await
            .expect_err("should report onboarding error");

        match err {
            AuthResolutionError::Application(ApplicationError::Onboarding(
                application::errors::OnboardingStatus::AccessPending { submitted_at, .. },
            )) => {
                assert!(submitted_at <= Utc::now());
            }
            other => panic!("expected onboarding pending, got {other:?}"),
        }

        // ADR-006: Strictly read-only verification
        assert_eq!(
            resolver
                .access_request_repo
                .write_call_count
                .load(Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn test_resolve_caller_onboarding_rejected() {
        let user_id = UserId::try_new("usr_david").unwrap();
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo {
                request: Some(AccessRequest {
                    id: AccessRequestId::new(Uuid::now_v7()),
                    user_id: user_id.clone(),
                    email: None,
                    name: None,
                    requested_at: Utc::now(),
                    status: AccessRequestStatus::Rejected {
                        reason: Some("account not verified".to_string()),
                    },
                    reviewed_by: None,
                    reviewed_at: None,
                    assigned_roles: Vec::new(),
                }),
                write_call_count: AtomicUsize::new(0),
            },
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_david",
            email: None,
            name: None,
        };

        let err = resolver
            .resolve_caller(credentials)
            .await
            .expect_err("should report onboarding error");

        match err {
            AuthResolutionError::Application(ApplicationError::Onboarding(
                application::errors::OnboardingStatus::AccessRejected { reason, .. },
            )) => {
                assert_eq!(reason, Some("account not verified".to_string()));
            }
            other => panic!("expected onboarding rejected, got {other:?}"),
        }

        // ADR-006: Strictly read-only verification
        assert_eq!(
            resolver
                .access_request_repo
                .write_call_count
                .load(Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn test_resolve_caller_onboarding_not_requested() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo {
                request: None,
                write_call_count: AtomicUsize::new(0),
            },
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_newbie",
            email: None,
            name: None,
        };

        let err = resolver
            .resolve_caller(credentials)
            .await
            .expect_err("should report onboarding not requested error");

        match err {
            AuthResolutionError::Application(ApplicationError::Onboarding(
                application::errors::OnboardingStatus::AccessNotRequested { user_id },
            )) => {
                assert_eq!(user_id.as_ref(), "usr_newbie");
            }
            other => panic!("expected onboarding not requested, got {other:?}"),
        }

        // ADR-006: Strictly read-only verification
        assert_eq!(
            resolver
                .access_request_repo
                .write_call_count
                .load(Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn test_resolve_caller_onboarding_approved_without_roles_fallback() {
        let user_id = UserId::try_new("usr_approved_no_roles").unwrap();
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo {
                request: Some(AccessRequest {
                    id: AccessRequestId::new(Uuid::now_v7()),
                    user_id: user_id.clone(),
                    email: None,
                    name: None,
                    requested_at: Utc::now(),
                    status: AccessRequestStatus::Approved,
                    reviewed_by: None,
                    reviewed_at: None,
                    assigned_roles: Vec::new(),
                }),
                write_call_count: AtomicUsize::new(0),
            },
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "usr_approved_no_roles",
            email: None,
            name: None,
        };

        let err = resolver
            .resolve_caller(credentials)
            .await
            .expect_err("should report access not requested when approved but no roles");

        match err {
            AuthResolutionError::Application(ApplicationError::Onboarding(
                application::errors::OnboardingStatus::AccessNotRequested { user_id },
            )) => {
                assert_eq!(user_id.as_ref(), "usr_approved_no_roles");
            }
            other => panic!("expected onboarding not requested, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_resolve_caller_rejects_bearer_token() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::BearerToken("jwt-token");
        let err = resolver
            .resolve_caller(credentials)
            .await
            .expect_err("should reject bearer token");

        assert!(matches!(
            err,
            AuthResolutionError::Auth(AuthError::MissingToken)
        ));
    }

    #[tokio::test]
    async fn test_resolve_caller_rejects_empty_user_id() {
        let resolver = CognitoEdgeAuthResolver::with_repositories(
            MockAccessRequestRepo::default(),
            MockAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let credentials = AuthCredentials::EdgeCognito {
            user_id: "   ",
            email: None,
            name: None,
        };

        let err = resolver
            .resolve_caller(credentials)
            .await
            .expect_err("should reject empty user_id");

        assert!(matches!(
            err,
            AuthResolutionError::Auth(AuthError::MissingToken)
        ));
    }
}
