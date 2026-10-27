//! Verified authentication claims and identity models.

use domain::auth::role::RoleId;
use domain::auth::user::{ServiceId, UserId};
use domain::common::{DisplayName, Email};

// ---------------------------------------------------------------------------
// Identity Types
// ---------------------------------------------------------------------------

/// Verified human identity extracted from an OIDC JWT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedUser {
    pub user_id: UserId,
    pub email: Option<Email>,
    pub name: Option<DisplayName>,
}

/// Discriminated caller identity distinguishing human users from machine services.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthenticatedIdentity {
    User(AuthenticatedUser),
    Service {
        service_id: ServiceId,
        roles: Vec<RoleId>,
    },
}

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

impl AuthenticatedUser {
    /// Creates a new authenticated user identity.
    #[must_use]
    pub const fn new(
        user_id: UserId,
        email: Option<Email>,
        name: Option<DisplayName>,
    ) -> Self {
        Self {
            user_id,
            email,
            name,
        }
    }
}

impl AuthenticatedIdentity {
    /// Returns the human user identity if this identity represents a user.
    #[must_use]
    pub const fn as_user(&self) -> Option<&AuthenticatedUser> {
        match self {
            Self::User(u) => Some(u),
            Self::Service { .. } => None,
        }
    }

    /// Returns the service id and roles if this identity represents a service.
    #[must_use]
    pub fn as_service(&self) -> Option<(&ServiceId, &[RoleId])> {
        match self {
            Self::User(_) => None,
            Self::Service { service_id, roles } => Some((service_id, roles.as_slice())),
        }
    }

    /// Returns `true` if this identity represents a human user.
    #[must_use]
    pub const fn is_user(&self) -> bool {
        matches!(self, Self::User(_))
    }

    /// Returns `true` if this identity represents a machine service.
    #[must_use]
    pub const fn is_service(&self) -> bool {
        matches!(self, Self::Service { .. })
    }
}

impl From<AuthenticatedUser> for AuthenticatedIdentity {
    fn from(user: AuthenticatedUser) -> Self {
        Self::User(user)
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_authenticated_user_creation() {
        let user_id = UserId::try_new("usr_123").expect("valid user id");
        let email = Email::try_new("alice@example.com").ok();
        let name = DisplayName::try_new("Alice Smith").ok();

        let user = AuthenticatedUser::new(user_id.clone(), email.clone(), name.clone());
        assert_eq!(user.user_id, user_id);
        assert_eq!(user.email, email);
        assert_eq!(user.name, name);
    }

    #[test]
    fn test_authenticated_identity_user_variant() {
        let user_id = UserId::try_new("usr_123").expect("valid user id");
        let user = AuthenticatedUser::new(user_id, None, None);
        let identity = AuthenticatedIdentity::from(user.clone());

        assert!(identity.is_user());
        assert!(!identity.is_service());
        assert_eq!(identity.as_user(), Some(&user));
        assert_eq!(identity.as_service(), None);
    }

    #[test]
    fn test_authenticated_identity_service_variant() {
        let service_id = ServiceId::try_new("backup-agent").expect("valid service id");
        let role_id = RoleId::new(Uuid::now_v7());
        let identity = AuthenticatedIdentity::Service {
            service_id: service_id.clone(),
            roles: vec![role_id],
        };

        assert!(!identity.is_user());
        assert!(identity.is_service());
        assert_eq!(identity.as_user(), None);

        let (svc, roles) = identity.as_service().expect("service identity");
        assert_eq!(svc, &service_id);
        assert_eq!(roles, &[role_id]);
    }
}
