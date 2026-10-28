//! Authentication context resolution and caller identity extraction.

use std::future::Future;
use std::sync::Arc;

use domain::auth::access_request::AccessRequestStatus;
use domain::auth::ports::{AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository};
use domain::auth::role::{RoleId, UserRoleAssignment};
use domain::auth::user::{ActorId, UserId};
use application::context::CallerContext;
use application::errors::ApplicationError;

use crate::auth::claims::AuthenticatedUser;
use crate::auth::error::{AuthError, AuthResolutionError};
use crate::auth::oidc::OidcTokenValidator;
use crate::auth::service_token::{ServiceTokenEntry, ServiceTokenRegistry};
use crate::persistence::{
    SqlxAccessRequestRepository, SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};

// ---------------------------------------------------------------------------
// Traits & Types
// ---------------------------------------------------------------------------

/// Port trait defining operations to resolve incoming credentials into caller identities.
pub trait AuthContextResolver: Send + Sync + 'static {
    /// Resolves and verifies the human user identity from the `Authorization` header.
    fn resolve_user_identity(
        &self,
        auth_header: Option<&str>,
    ) -> impl Future<Output = Result<AuthenticatedUser, AuthResolutionError>> + Send;

    /// Resolves the authenticated caller and their roles, checking onboarding status.
    fn resolve_caller(
        &self,
        auth_header: Option<&str>,
    ) -> impl Future<Output = Result<CallerContext, AuthResolutionError>> + Send;
}

/// Generic authentication resolver orchestrating token verification, onboarding, and RBAC.
pub struct DefaultAuthResolver<A, U, R> {
    oidc_validator: Arc<OidcTokenValidator>,
    service_tokens: Arc<ServiceTokenRegistry>,
    access_request_repo: A,
    assignment_repo: U,
    role_repo: R,
}

/// Production database-backed authentication resolver.
pub type SqlxAuthResolver = DefaultAuthResolver<
    SqlxAccessRequestRepository,
    SqlxUserRoleAssignmentRepository,
    SqlxRoleRepository,
>;

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

impl<A, U, R> DefaultAuthResolver<A, U, R> {
    /// Creates a new authentication resolver with the given dependencies.
    pub fn new(
        oidc_validator: Arc<OidcTokenValidator>,
        service_tokens: Arc<ServiceTokenRegistry>,
        access_request_repo: A,
        assignment_repo: U,
        role_repo: R,
    ) -> Self {
        Self {
            oidc_validator,
            service_tokens,
            access_request_repo,
            assignment_repo,
            role_repo,
        }
    }
}

impl SqlxAuthResolver {
    /// Creates a database-backed resolver using repositories bound to the given connection pool.
    #[must_use]
    pub fn from_pool(
        pool: sqlx::PgPool,
        oidc_validator: Arc<OidcTokenValidator>,
        service_tokens: Arc<ServiceTokenRegistry>,
    ) -> Self {
        Self::new(
            oidc_validator,
            service_tokens,
            SqlxAccessRequestRepository::new(pool.clone()),
            SqlxUserRoleAssignmentRepository::new(pool.clone()),
            SqlxRoleRepository::new(pool),
        )
    }
}

impl<A, U, R> AuthContextResolver for DefaultAuthResolver<A, U, R>
where
    A: AccessRequestRepository + 'static,
    U: UserRoleAssignmentRepository + 'static,
    R: RoleRepository + 'static,
{
    async fn resolve_user_identity(
        &self,
        auth_header: Option<&str>,
    ) -> Result<AuthenticatedUser, AuthResolutionError> {
        let token = extract_bearer_token(auth_header)?;
        let user = self.oidc_validator.verify_token(token)?;
        Ok(user)
    }

    async fn resolve_caller(
        &self,
        auth_header: Option<&str>,
    ) -> Result<CallerContext, AuthResolutionError> {
        let token = extract_bearer_token(auth_header)?;

        if let Some(service_entry) = self.service_tokens.verify(token) {
            return self.resolve_service_caller(service_entry).await;
        }

        let user = self.oidc_validator.verify_token(token)?;

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

impl<A, U, R> DefaultAuthResolver<A, U, R>
where
    A: AccessRequestRepository,
    R: RoleRepository,
{
    async fn resolve_service_caller(
        &self,
        entry: &ServiceTokenEntry,
    ) -> Result<CallerContext, AuthResolutionError> {
        let roles = self
            .role_repo
            .find_by_ids(&entry.role_ids)
            .await
            .map_err(ApplicationError::Domain)?;
        Ok(CallerContext::new(
            ActorId::Service(entry.service_id.clone()),
            roles,
        ))
    }

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
            AccessRequestStatus::Approved => {
                ApplicationError::access_not_requested(user_id).into()
            }
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
// Public Functions
// ---------------------------------------------------------------------------

/// Extracts the raw bearer token string from an `Authorization` header value.
///
/// # Errors
/// Returns [`AuthError::MissingToken`] if the header is missing or blank,
/// or [`AuthError::MalformedHeader`] if the scheme is not `Bearer <token>`.
pub fn extract_bearer_token(auth_header: Option<&str>) -> Result<&str, AuthError> {
    let header = auth_header.ok_or(AuthError::MissingToken)?;
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
    Ok(token)
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
    use domain::auth::user::ServiceId;
    use domain::errors::DomainError;
    use uuid::Uuid;

    #[derive(Default)]
    struct MockAccessRequestRepo {
        request: Option<AccessRequest>,
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
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockUserRoleAssignmentRepo {
        assignments: Vec<UserRoleAssignment>,
    }

    impl UserRoleAssignmentRepository for MockUserRoleAssignmentRepo {
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
            Ok(())
        }

        async fn save_all(&self, _assignments: &[UserRoleAssignment]) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: UserRoleAssignmentId) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct MockRoleRepo {
        roles: Vec<Role>,
    }

    impl RoleRepository for MockRoleRepo {
        async fn find_by_id(&self, _id: RoleId) -> Result<Option<Role>, DomainError> {
            Ok(None)
        }

        async fn find_by_ids(&self, _ids: &[RoleId]) -> Result<Vec<Role>, DomainError> {
            Ok(self.roles.clone())
        }

        async fn find_by_name(&self, _name: &str) -> Result<Option<Role>, DomainError> {
            Ok(None)
        }

        async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
            Ok(Vec::new())
        }

        async fn save(&self, _role: &Role) -> Result<(), DomainError> {
            Ok(())
        }
    }

    #[test]
    fn test_extract_bearer_token() {
        assert_eq!(extract_bearer_token(None), Err(AuthError::MissingToken));
        assert_eq!(extract_bearer_token(Some("")), Err(AuthError::MissingToken));
        assert_eq!(
            extract_bearer_token(Some("   ")),
            Err(AuthError::MissingToken)
        );
        assert_eq!(
            extract_bearer_token(Some("Basic abc")),
            Err(AuthError::MalformedHeader)
        );
        assert_eq!(
            extract_bearer_token(Some("Bearer")),
            Err(AuthError::MalformedHeader)
        );
        assert_eq!(
            extract_bearer_token(Some("Bearer ")),
            Err(AuthError::MalformedHeader)
        );
        assert_eq!(
            extract_bearer_token(Some("Bearer    ")),
            Err(AuthError::MalformedHeader)
        );
        assert_eq!(
            extract_bearer_token(Some("Bearer my-secret-token")),
            Ok("my-secret-token")
        );
        assert_eq!(
            extract_bearer_token(Some("  Bearer my-secret-token  ")),
            Ok("my-secret-token")
        );
    }

    #[tokio::test]
    async fn test_resolve_service_caller() {
        let secret = b"my-secret-key-that-is-long-enough-123";
        let validator = Arc::new(OidcTokenValidator::new_hs256(secret, None, None));

        let mut token_registry = ServiceTokenRegistry::new();
        let svc_id = ServiceId::try_new("indexer").expect("valid service");
        let role_id = RoleId::new(Uuid::now_v7());
        token_registry.register("svc-token-123", svc_id.clone(), vec![role_id]);
        let token_registry = Arc::new(token_registry);

        let test_role = Role {
            id: role_id,
            name: "service_role".into(),
            description: None,
            permissions: vec![],
        };
        let role_repo = MockRoleRepo {
            roles: vec![test_role],
        };

        let resolver = DefaultAuthResolver::new(
            validator,
            token_registry,
            MockAccessRequestRepo::default(),
            MockUserRoleAssignmentRepo::default(),
            role_repo,
        );

        let caller = resolver
            .resolve_caller(Some("Bearer svc-token-123"))
            .await
            .expect("service caller resolved");

        assert_eq!(caller.actor, ActorId::Service(svc_id));
        assert_eq!(caller.roles.len(), 1);
        assert_eq!(caller.roles[0].name, "service_role");
    }

    #[tokio::test]
    async fn test_resolve_caller_onboarding_not_requested() {
        let secret = b"my-secret-key-that-is-long-enough-123";
        let validator = Arc::new(OidcTokenValidator::new_hs256(secret, None, None));
        let token = OidcTokenValidator::encode_hs256_token(
            secret,
            "usr_newbie",
            None,
            None,
            usize::MAX,
        )
        .expect("token");

        let resolver = DefaultAuthResolver::new(
            validator,
            Arc::new(ServiceTokenRegistry::new()),
            MockAccessRequestRepo::default(),
            MockUserRoleAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let err = resolver
            .resolve_caller(Some(&format!("Bearer {token}")))
            .await
            .expect_err("must fail onboarding");

        match err {
            AuthResolutionError::Application(ApplicationError::Onboarding(
                application::errors::OnboardingStatus::AccessNotRequested { user_id },
            )) => {
                assert_eq!(user_id.as_ref(), "usr_newbie");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_resolve_caller_with_roles() {
        let secret = b"my-secret-key-that-is-long-enough-123";
        let validator = Arc::new(OidcTokenValidator::new_hs256(secret, None, None));
        let token = OidcTokenValidator::encode_hs256_token(
            secret,
            "usr_alice",
            None,
            None,
            usize::MAX,
        )
        .expect("token");

        let user_id = UserId::try_new("usr_alice").expect("valid user");
        let role_id = RoleId::new(Uuid::now_v7());
        let assignment = UserRoleAssignment {
            id: UserRoleAssignmentId::new(Uuid::now_v7()),
            user_id: user_id.clone(),
            role_id,
            granted_at: Utc::now(),
            granted_by: None,
        };
        let test_role = Role {
            id: role_id,
            name: "editor".into(),
            description: None,
            permissions: vec![],
        };

        let resolver = DefaultAuthResolver::new(
            validator,
            Arc::new(ServiceTokenRegistry::new()),
            MockAccessRequestRepo::default(),
            MockUserRoleAssignmentRepo {
                assignments: vec![assignment],
            },
            MockRoleRepo {
                roles: vec![test_role],
            },
        );

        let caller = resolver
            .resolve_caller(Some(&format!("Bearer {token}")))
            .await
            .expect("caller resolved");

        assert_eq!(caller.actor, ActorId::User(user_id));
        assert_eq!(caller.roles.len(), 1);
        assert_eq!(caller.roles[0].name, "editor");
    }

    #[tokio::test]
    async fn test_resolve_user_identity() {
        let secret = b"my-secret-key-that-is-long-enough-123";
        let validator = Arc::new(OidcTokenValidator::new_hs256(secret, None, None));
        let token = OidcTokenValidator::encode_hs256_token(
            secret,
            "usr_applicant",
            Some("app@example.com"),
            Some("Applicant"),
            usize::MAX,
        )
        .expect("token");

        let resolver = DefaultAuthResolver::new(
            validator,
            Arc::new(ServiceTokenRegistry::new()),
            MockAccessRequestRepo::default(),
            MockUserRoleAssignmentRepo::default(),
            MockRoleRepo::default(),
        );

        let user = resolver
            .resolve_user_identity(Some(&format!("Bearer {token}")))
            .await
            .expect("identity resolved");

        assert_eq!(user.user_id.as_ref(), "usr_applicant");
        assert_eq!(
            user.email.as_ref().map(AsRef::as_ref),
            Some("app@example.com")
        );
        assert_eq!(user.name.as_ref().map(AsRef::as_ref), Some("Applicant"));
    }
}
