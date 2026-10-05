use super::role::{Permission, Role};
use super::user::UserId;
use crate::content::instance::DocumentInstance;

pub struct AuthorizationService;

impl AuthorizationService {
    /// Evaluates access authorization using the precedence rule:
    /// 1. Owner rule (creators can read, update, and delete their own instances without a role).
    ///    Publishing always requires explicit RBAC permission, enabling editorial workflows
    ///    where authors cannot self-publish.
    /// 2. RBAC (evaluation across all assigned roles).
    /// 3. Default DENY.
    pub fn can(
        user_id: &UserId,
        action: &Permission,
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
                Permission::ReadDocument(_)
                | Permission::UpdateDocument(_)
                | Permission::DeleteDocument(_) => return true,
                _ => {}
            }
        }

        // 2. RBAC check
        roles.iter().any(|role| role.has_permission(action))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    use crate::auth::role::RoleId;
    use crate::schema::DocumentTypeId;
    use crate::test_support::{
        admin_role, fixture_document_instance, test_doc_type_id, test_user_id,
    };

    fn make_test_fixture() -> (UserId, UserId, DocumentTypeId, DocumentInstance) {
        let owner = test_user_id("owner_user");
        let other = test_user_id("other_user");
        let type_id = test_doc_type_id("article");
        let instance = fixture_document_instance("article", Some("owner_user"));
        (owner, other, type_id, instance)
    }

    #[test]
    fn test_owner_allowed_read_update_and_delete() {
        let (owner, _, type_id, instance) = make_test_fixture();
        // Owner may read, update, and delete without any role
        assert!(AuthorizationService::can(
            &owner,
            &Permission::ReadDocument(Some(type_id.clone())),
            Some(&instance),
            &[]
        ));
        assert!(AuthorizationService::can(
            &owner,
            &Permission::UpdateDocument(Some(type_id.clone())),
            Some(&instance),
            &[]
        ));
        assert!(AuthorizationService::can(
            &owner,
            &Permission::DeleteDocument(Some(type_id)),
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
            &Permission::PublishDocument(Some(type_id)),
            Some(&instance),
            &[]
        ));
    }

    #[test]
    fn test_rbac_explicit_permission_granted() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::ReadDocument(Some(type_id.clone()));
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "reader".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(Some(type_id))],
        };

        assert!(AuthorizationService::can(
            &other,
            &action,
            Some(&instance),
            &[role]
        ));
    }

    #[test]
    fn test_rbac_wildcard_permission() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::ReadDocument(Some(type_id));
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "global_reader".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(None)], // wildcard
        };

        assert!(AuthorizationService::can(
            &other,
            &action,
            Some(&instance),
            &[role]
        ));
    }

    #[test]
    fn test_rbac_wrong_permission_denied() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::DeleteDocument(Some(type_id.clone()));
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "reader".into(),
            permissions: vec![Permission::ReadDocument(Some(type_id))],
            description: None,
        };

        assert!(!AuthorizationService::can(
            &other,
            &action,
            Some(&instance),
            &[role]
        ));
    }

    #[test]
    fn test_no_roles_denied() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::ReadDocument(Some(type_id));
        assert!(!AuthorizationService::can(
            &other,
            &action,
            Some(&instance),
            &[]
        ));
    }

    #[test]
    fn test_admin_all_permissions() {
        let (_, other, type_id, instance) = make_test_fixture();
        let action = Permission::DeleteDocument(Some(type_id));
        assert!(AuthorizationService::can(
            &other,
            &action,
            Some(&instance),
            &[admin_role()]
        ));
    }
}
