//! Security and caller identity context.

use domain::auth::{ActorId, AuthorizationService, Permission, Role, RoleId, ServiceId, UserId};
use domain::content::DocumentInstance;

use crate::errors::ApplicationError;

/// Authenticated caller context carrying the verified identity and roles of the actor.
#[derive(Debug, Clone)]
pub struct CallerContext {
    /// The authenticated actor (human user, service/API token, or internal system).
    pub actor: ActorId,
    /// Roles assigned to the actor.
    pub roles: Vec<Role>,
}

impl CallerContext {
    /// Creates a new `CallerContext` for an authenticated actor (user, service, or system) and their roles.
    #[must_use]
    pub fn new(actor: impl Into<ActorId>, roles: Vec<Role>) -> Self {
        Self {
            actor: actor.into(),
            roles,
        }
    }

    /// Creates a new `CallerContext` for a service actor.
    #[must_use]
    pub fn service(service_id: ServiceId, roles: Vec<Role>) -> Self {
        Self {
            actor: ActorId::Service(service_id),
            roles,
        }
    }

    /// Creates an internal system caller context with administrative privileges.
    #[must_use]
    pub fn system() -> Self {
        let admin_role = Role {
            id: RoleId::new(uuid::Uuid::nil()),
            name: "system_admin".to_string(),
            description: Some("System internal role with full privileges".to_string()),
            permissions: vec![
                Permission::ManageSchema,
                Permission::ManageRoles,
                Permission::ManageUsers,
                Permission::CreateDocument(None),
                Permission::ReadDocument(None),
                Permission::UpdateDocument(None),
                Permission::DeleteDocument(None),
                Permission::PublishDocument(None),
            ],
        };
        Self {
            actor: ActorId::System,
            roles: vec![admin_role],
        }
    }

    /// Returns the associated user ID if the actor is a human user.
    #[must_use]
    pub fn user_id(&self) -> Option<&UserId> {
        self.actor.as_user()
    }

    /// Evaluates whether the caller has permission to perform `action` on the optional `instance`.
    ///
    /// Precedence:
    /// 1. System bypass (system actor has all permissions).
    /// 2. Owner rule (creators have full rights to their own instances, excluding publish).
    /// 3. RBAC (across all assigned roles).
    /// 4. Default DENY.
    #[must_use]
    pub fn can(&self, action: &Permission, instance: Option<&DocumentInstance>) -> bool {
        AuthorizationService::can_actor(&self.actor, action, instance, &self.roles)
    }

    /// Enforces that the caller has permission to perform `action` on the optional `instance`.
    ///
    /// # Errors
    /// Returns [`ApplicationError::Unauthorized`] if the caller lacks permission.
    pub fn check_permission(
        &self,
        action: &Permission,
        instance: Option<&DocumentInstance>,
    ) -> Result<(), ApplicationError> {
        if self.can(action, instance) {
            Ok(())
        } else {
            Err(ApplicationError::Unauthorized {
                actor: self.actor.clone(),
                action: action.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::schema::DocumentTypeId;
    use uuid::Uuid;

    fn make_test_instance(owner: &UserId) -> (DocumentTypeId, DocumentInstance) {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let instance = DocumentInstance::new(type_id.clone(), owner.clone(), Utc::now());
        (type_id, instance)
    }

    #[test]
    fn test_system_caller_has_all_permissions() {
        let ctx = CallerContext::system();
        let type_id = DocumentTypeId::try_new("article").unwrap();

        assert!(ctx.can(&Permission::ManageSchema, None));
        assert!(ctx.can(&Permission::ManageRoles, None));
        assert!(ctx.can(&Permission::ManageUsers, None));
        assert!(ctx.can(&Permission::CreateDocument(Some(type_id.clone())), None));
        assert!(ctx.can(&Permission::PublishDocument(Some(type_id)), None));

        assert!(
            ctx.check_permission(&Permission::ManageSchema, None)
                .is_ok()
        );
    }

    #[test]
    fn test_check_permission_denied_returns_unauthorized() {
        let user_id = UserId::try_new("user-reader").expect("valid user");
        let ctx = CallerContext::new(user_id.clone(), Vec::new());
        let type_id = DocumentTypeId::try_new("article").unwrap();

        let res = ctx.check_permission(&Permission::DeleteDocument(Some(type_id.clone())), None);
        match res {
            Err(ApplicationError::Unauthorized { actor, action }) => {
                assert_eq!(actor, ActorId::User(user_id));
                assert_eq!(action, Permission::DeleteDocument(Some(type_id)));
            }
            _ => panic!("expected Unauthorized error"),
        }
    }

    #[test]
    fn test_owner_rule_allows_read_update_delete_without_roles() {
        let owner = UserId::try_new("user-owner").expect("valid user");
        let ctx = CallerContext::new(owner.clone(), Vec::new());
        let (type_id, instance) = make_test_instance(&owner);

        // Owner may read and update without any role
        assert!(ctx.can(
            &Permission::UpdateDocument(Some(type_id.clone())),
            Some(&instance)
        ));
        assert!(ctx.can(
            &Permission::ReadDocument(Some(type_id.clone())),
            Some(&instance)
        ));
        assert!(
            ctx.check_permission(
                &Permission::UpdateDocument(Some(type_id.clone())),
                Some(&instance)
            )
            .is_ok()
        );
        assert!(
            ctx.check_permission(
                &Permission::DeleteDocument(Some(type_id.clone())),
                Some(&instance)
            )
            .is_ok()
        );
        // Publish require explicit RBAC — owner rule does not apply
        assert!(!ctx.can(
            &Permission::PublishDocument(Some(type_id.clone())),
            Some(&instance)
        ))
    }

    #[test]
    fn test_rbac_allows_action_with_role() {
        let user = UserId::try_new("user-editor").expect("valid user");
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "editor".to_string(),
            description: None,
            permissions: vec![Permission::CreateDocument(Some(type_id.clone()))],
        };
        let ctx = CallerContext::new(user, vec![role]);

        assert!(ctx.can(&Permission::CreateDocument(Some(type_id.clone())), None));
        assert!(!ctx.can(&Permission::DeleteDocument(Some(type_id)), None));
    }

    #[test]
    fn test_service_caller_rbac() {
        let service_id = ServiceId::try_new("ci-pipeline").expect("valid service");
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "ingestion".to_string(),
            description: None,
            permissions: vec![Permission::CreateDocument(Some(type_id.clone()))],
        };
        let ctx = CallerContext::service(service_id.clone(), vec![role]);

        assert_eq!(ctx.user_id(), None);
        assert!(ctx.can(&Permission::CreateDocument(Some(type_id.clone())), None));
        assert!(!ctx.can(&Permission::DeleteDocument(Some(type_id.clone())), None));

        let res = ctx.check_permission(&Permission::DeleteDocument(Some(type_id.clone())), None);
        match res {
            Err(ApplicationError::Unauthorized { actor, action }) => {
                assert_eq!(actor, ActorId::Service(service_id));
                assert_eq!(action, Permission::DeleteDocument(Some(type_id)));
            }
            _ => panic!("expected Unauthorized error"),
        }
    }
}
