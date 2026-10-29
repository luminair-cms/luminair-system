//! Security and caller identity context.

use domain::auth::{
    ActorId, AuthorizationService, PermissionCheck, Role, RoleId, ServiceId, UserId,
};
use domain::content::DocumentInstance;
use domain::schema::DocumentTypeId;

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
        use domain::auth::Permission;
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
    pub fn can(&self, action: &PermissionCheck<'_>, instance: Option<&DocumentInstance>) -> bool {
        AuthorizationService::can_actor(&self.actor, action, instance, &self.roles)
    }

    /// Enforces that the caller has permission to perform `action` on the optional `instance`
    /// without any allocations on the authorized path.
    ///
    /// # Errors
    /// Returns [`ApplicationError::Unauthorized`] if the caller lacks permission.
    pub fn check_permission(
        &self,
        action: &PermissionCheck<'_>,
        instance: Option<&DocumentInstance>,
    ) -> Result<(), ApplicationError> {
        if self.can(action, instance) {
            Ok(())
        } else {
            Err(ApplicationError::Unauthorized {
                actor: self.actor.clone(),
                action: action.to_permission(),
            })
        }
    }

    /// Checks if the caller has permission to create a document of the specified type.
    pub fn check_create_document(&self, type_id: &DocumentTypeId) -> Result<(), ApplicationError> {
        self.check_permission(&PermissionCheck::create_document(type_id), None)
    }

    /// Checks if the caller has permission to read a document of the specified type.
    pub fn check_read_document(
        &self,
        type_id: &DocumentTypeId,
        instance: Option<&DocumentInstance>,
    ) -> Result<(), ApplicationError> {
        self.check_permission(&PermissionCheck::read_document(type_id), instance)
    }

    /// Checks if the caller has permission to update a document of the specified type.
    pub fn check_update_document(
        &self,
        type_id: &DocumentTypeId,
        instance: Option<&DocumentInstance>,
    ) -> Result<(), ApplicationError> {
        self.check_permission(&PermissionCheck::update_document(type_id), instance)
    }

    /// Checks if the caller has permission to delete a document of the specified type.
    pub fn check_delete_document(
        &self,
        type_id: &DocumentTypeId,
        instance: Option<&DocumentInstance>,
    ) -> Result<(), ApplicationError> {
        self.check_permission(&PermissionCheck::delete_document(type_id), instance)
    }

    /// Checks if the caller has permission to publish a document of the specified type.
    pub fn check_publish_document(
        &self,
        type_id: &DocumentTypeId,
        instance: Option<&DocumentInstance>,
    ) -> Result<(), ApplicationError> {
        self.check_permission(&PermissionCheck::publish_document(type_id), instance)
    }

    /// Checks if the caller has permission to manage schema.
    pub fn check_manage_schema(&self) -> Result<(), ApplicationError> {
        self.check_permission(&PermissionCheck::ManageSchema, None)
    }

    /// Checks if the caller has permission to manage roles.
    pub fn check_manage_roles(&self) -> Result<(), ApplicationError> {
        self.check_permission(&PermissionCheck::ManageRoles, None)
    }

    /// Checks if the caller has permission to manage users.
    pub fn check_manage_users(&self) -> Result<(), ApplicationError> {
        self.check_permission(&PermissionCheck::ManageUsers, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::auth::Permission;
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

        assert!(ctx.can(&PermissionCheck::ManageSchema, None));
        assert!(ctx.can(&PermissionCheck::ManageRoles, None));
        assert!(ctx.can(&PermissionCheck::ManageUsers, None));
        assert!(ctx.can(&PermissionCheck::create_document(&type_id), None));
        assert!(ctx.can(&PermissionCheck::publish_document(&type_id), None));

        assert!(ctx.check_manage_schema().is_ok());
        assert!(ctx.check_manage_roles().is_ok());
        assert!(ctx.check_manage_users().is_ok());
        assert!(ctx.check_create_document(&type_id).is_ok());
        assert!(ctx.check_read_document(&type_id, None).is_ok());
        assert!(ctx.check_update_document(&type_id, None).is_ok());
        assert!(ctx.check_delete_document(&type_id, None).is_ok());
        assert!(ctx.check_publish_document(&type_id, None).is_ok());
    }

    #[test]
    fn test_check_permission_denied_returns_unauthorized() {
        let user_id = UserId::try_new("user-reader").expect("valid user");
        let ctx = CallerContext::new(user_id.clone(), Vec::new());
        let type_id = DocumentTypeId::try_new("article").unwrap();

        let res = ctx.check_delete_document(&type_id, None);
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
        assert!(ctx.can(&PermissionCheck::update_document(&type_id), Some(&instance)));
        assert!(ctx.can(&PermissionCheck::read_document(&type_id), Some(&instance)));
        assert!(ctx.check_update_document(&type_id, Some(&instance)).is_ok());
        assert!(ctx.check_delete_document(&type_id, Some(&instance)).is_ok());
        assert!(ctx.check_read_document(&type_id, Some(&instance)).is_ok());

        // Publish require explicit RBAC — owner rule does not apply
        assert!(!ctx.can(
            &PermissionCheck::publish_document(&type_id),
            Some(&instance)
        ));
        assert!(
            ctx.check_publish_document(&type_id, Some(&instance))
                .is_err()
        );
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

        assert!(ctx.check_create_document(&type_id).is_ok());
        assert!(ctx.check_delete_document(&type_id, None).is_err());
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
        assert!(ctx.check_create_document(&type_id).is_ok());
        assert!(ctx.check_delete_document(&type_id, None).is_err());

        let res = ctx.check_delete_document(&type_id, None);
        match res {
            Err(ApplicationError::Unauthorized { actor, action }) => {
                assert_eq!(actor, ActorId::Service(service_id));
                assert_eq!(action, Permission::DeleteDocument(Some(type_id)));
            }
            _ => panic!("expected Unauthorized error"),
        }
    }
}
