use super::role::{PermissionCheck, Role};
use super::user::{ActorId, UserId};
use crate::content::instance::DocumentInstance;

pub struct AuthorizationService;

impl AuthorizationService {
    /// Evaluates access authorization using the precedence rule:
    /// 1. Owner rule (creators can read, update, and delete their own instances without a role).
    ///    Publishing always requires explicit RBAC permission, enabling editorial workflows
    ///    where authors cannot self-publish.
    /// 2. RBAC (evaluation across all assigned roles).
    /// 3. Default DENY.
    #[must_use]
    pub fn can(
        user_id: &UserId,
        action: &PermissionCheck<'_>,
        instance: Option<&DocumentInstance>,
        roles: &[Role],
    ) -> bool {
        // 1. Special Owner Rule: Owner may read, update, and delete their own instance without a role.
        //    Publish always requires explicit RBAC permission, enabling editorial
        //    workflows where authors cannot self-publish.
        if let Some(inst) = instance
            && inst.is_owned_by(user_id)
        {
            match action {
                PermissionCheck::ReadDocument(_)
                | PermissionCheck::UpdateDocument(_)
                | PermissionCheck::DeleteDocument(_) => return true,
                _ => {}
            }
        }

        // 2. RBAC check
        roles.iter().any(|role| role.has_permission(action))
    }

    /// Evaluates access authorization for an `ActorId`:
    /// - If `actor.is_system()`, returns `true`.
    /// - If `actor.as_user()`, delegates to `Self::can(user_id, action, instance, roles)`.
    /// - If `ActorId::Service(_)`, evaluates RBAC across `roles`.
    #[must_use]
    pub fn can_actor(
        actor: &ActorId,
        action: &PermissionCheck<'_>,
        instance: Option<&DocumentInstance>,
        roles: &[Role],
    ) -> bool {
        if actor.is_system() {
            return true;
        }

        if let Some(user_id) = actor.as_user() {
            return Self::can(user_id, action, instance, roles);
        }

        if actor.as_service().is_some() {
            return roles.iter().any(|role| role.has_permission(action));
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    use crate::auth::role::{Permission, RoleId};
    use crate::schema::DocumentTypeId;
    use crate::test_support::{
        admin_role, create_doc_type_id, create_user_id, fixture_document_instance,
    };

    fn make_test_fixture() -> (UserId, UserId, DocumentTypeId, DocumentInstance) {
        let owner = create_user_id("owner_user");
        let other = create_user_id("other_user");
        let type_id = create_doc_type_id("article");
        let instance = fixture_document_instance("article", Some("owner_user"));
        (owner, other, type_id, instance)
    }

    #[test]
    fn test_owner_allowed_read_update_and_delete() {
        let (owner, _, type_id, instance) = make_test_fixture();
        // Owner may read, update, and delete without any role
        assert!(AuthorizationService::can(
            &owner,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            &[]
        ));
        assert!(AuthorizationService::can(
            &owner,
            &PermissionCheck::update_document(&type_id),
            Some(&instance),
            &[]
        ));
        assert!(AuthorizationService::can(
            &owner,
            &PermissionCheck::delete_document(&type_id),
            Some(&instance),
            &[]
        ));
    }

    #[test]
    fn test_owner_denied_publish_without_role() {
        let (owner, _, type_id, instance) = make_test_fixture();
        // Owner cannot publish without an explicit role
        assert!(!AuthorizationService::can(
            &owner,
            &PermissionCheck::publish_document(&type_id),
            Some(&instance),
            &[]
        ));
    }

    #[test]
    fn test_rbac_explicit_permission_granted() {
        let (_, other, type_id, instance) = make_test_fixture();
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "reader".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(Some(type_id.clone()))],
        };

        assert!(AuthorizationService::can(
            &other,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            std::slice::from_ref(&role)
        ));
    }

    #[test]
    fn test_rbac_wildcard_permission() {
        let (_, other, type_id, instance) = make_test_fixture();
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "global_reader".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(None)], // wildcard
        };

        assert!(AuthorizationService::can(
            &other,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            std::slice::from_ref(&role)
        ));
    }

    #[test]
    fn test_rbac_wrong_permission_denied() {
        let (_, other, type_id, instance) = make_test_fixture();
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "reader".into(),
            permissions: vec![Permission::ReadDocument(Some(type_id.clone()))],
            description: None,
        };

        assert!(!AuthorizationService::can(
            &other,
            &PermissionCheck::delete_document(&type_id),
            Some(&instance),
            std::slice::from_ref(&role)
        ));
    }

    #[test]
    fn test_no_roles_denied() {
        let (_, other, type_id, instance) = make_test_fixture();
        assert!(!AuthorizationService::can(
            &other,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            &[]
        ));
    }

    #[test]
    fn test_admin_all_permissions() {
        let (_, other, type_id, instance) = make_test_fixture();
        assert!(AuthorizationService::can(
            &other,
            &PermissionCheck::delete_document(&type_id),
            Some(&instance),
            &[admin_role()]
        ));
    }

    #[test]
    fn test_can_actor_system_allowed_unconditionally() {
        let (_, _, type_id, instance) = make_test_fixture();
        // System actor is allowed even with no roles and publishing action
        assert!(AuthorizationService::can_actor(
            &ActorId::System,
            &PermissionCheck::publish_document(&type_id),
            Some(&instance),
            &[]
        ));
        assert!(AuthorizationService::can_actor(
            &ActorId::System,
            &PermissionCheck::publish_document(&type_id),
            None,
            &[]
        ));
    }

    #[test]
    fn test_can_actor_user_delegation() {
        let (owner, other, type_id, instance) = make_test_fixture();
        let owner_actor = ActorId::User(owner);
        let other_actor = ActorId::User(other);

        // Owner can read/update without role
        assert!(AuthorizationService::can_actor(
            &owner_actor,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            &[]
        ));
        // Owner cannot publish without role
        assert!(!AuthorizationService::can_actor(
            &owner_actor,
            &PermissionCheck::publish_document(&type_id),
            Some(&instance),
            &[]
        ));

        // Other cannot read without role
        assert!(!AuthorizationService::can_actor(
            &other_actor,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            &[]
        ));
        // Other with role can read
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "reader".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(Some(type_id.clone()))],
        };
        assert!(AuthorizationService::can_actor(
            &other_actor,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            std::slice::from_ref(&role)
        ));
    }

    #[test]
    fn test_can_actor_service_evaluates_rbac() {
        let (_, _, type_id, instance) = make_test_fixture();
        let service = crate::test_support::create_service_id("export-worker");
        let service_actor = ActorId::Service(service);

        // Service has no owner privileges even if instance is present
        assert!(!AuthorizationService::can_actor(
            &service_actor,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            &[]
        ));

        // Service with matching role is allowed
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "worker".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(Some(type_id.clone()))],
        };
        assert!(AuthorizationService::can_actor(
            &service_actor,
            &PermissionCheck::read_document(&type_id),
            Some(&instance),
            std::slice::from_ref(&role)
        ));

        // Service with admin role is allowed
        assert!(AuthorizationService::can_actor(
            &service_actor,
            &PermissionCheck::publish_document(&type_id),
            Some(&instance),
            &[admin_role()]
        ));
    }
}
