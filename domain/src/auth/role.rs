use chrono::{DateTime, Utc};
use nutype::nutype;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::user::{ActorId, UserId};
use crate::schema::DocumentTypeId;

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct RoleId(Uuid);

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct UserRoleAssignmentId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    ManageSchema,
    CreateDocument(Option<DocumentTypeId>),
    ReadDocument(Option<DocumentTypeId>),
    UpdateDocument(Option<DocumentTypeId>),
    DeleteDocument(Option<DocumentTypeId>),
    PublishDocument(Option<DocumentTypeId>),
    ManageRoles,
    ManageUsers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: RoleId,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRoleAssignment {
    pub id: UserRoleAssignmentId,
    pub user_id: UserId,
    pub role_id: RoleId,
    pub granted_at: DateTime<Utc>,
    pub granted_by: Option<ActorId>, // None = system / bootstrap grant
}

impl Role {
    #[must_use]
    pub fn has_permission(&self, action: &Permission) -> bool {
        self.permissions.iter().any(|p| p.matches(action))
    }
}

impl Permission {
    /// Checks if this permission satisfies the requested action.
    /// `None` indicates a wildcard granting access to all document types.
    #[must_use]
    pub fn matches(&self, action: &Permission) -> bool {
        match (self, action) {
            (Permission::ManageSchema, Permission::ManageSchema)
            | (Permission::ManageRoles, Permission::ManageRoles)
            | (Permission::ManageUsers, Permission::ManageUsers)
            | (Permission::CreateDocument(None), Permission::CreateDocument(_))
            | (Permission::ReadDocument(None), Permission::ReadDocument(_))
            | (Permission::UpdateDocument(None), Permission::UpdateDocument(_))
            | (Permission::DeleteDocument(None), Permission::DeleteDocument(_))
            | (Permission::PublishDocument(None), Permission::PublishDocument(_)) => true,
            (Permission::CreateDocument(Some(a)), Permission::CreateDocument(Some(b)))
            | (Permission::ReadDocument(Some(a)), Permission::ReadDocument(Some(b)))
            | (Permission::UpdateDocument(Some(a)), Permission::UpdateDocument(Some(b)))
            | (Permission::DeleteDocument(Some(a)), Permission::DeleteDocument(Some(b)))
            | (Permission::PublishDocument(Some(a)), Permission::PublishDocument(Some(b))) => {
                a == b
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::create_doc_type_id;

    #[test]
    fn test_permission_matches_wildcard_and_exact() {
        let type_a = create_doc_type_id("article");
        let type_b = create_doc_type_id("author");

        let wildcard = Permission::ReadDocument(None);
        let exact_a = Permission::ReadDocument(Some(type_a));
        let exact_b = Permission::ReadDocument(Some(type_b));

        // Wildcard matches any type
        assert!(wildcard.matches(&exact_a));
        assert!(wildcard.matches(&exact_b));

        // Exact matches only same type
        assert!(exact_a.matches(&exact_a));
        assert!(!exact_a.matches(&exact_b));
        assert!(!exact_a.matches(&wildcard));

        // Different actions do not match
        assert!(!Permission::ManageSchema.matches(&exact_a));
        assert!(!Permission::CreateDocument(None).matches(&exact_a));
    }

    #[test]
    fn test_permission_matches_all_variants_wildcards_and_exact() {
        let doc_type = create_doc_type_id("article");
        let other_type = create_doc_type_id("author");

        // System-level permissions
        assert!(Permission::ManageSchema.matches(&Permission::ManageSchema));
        assert!(!Permission::ManageSchema.matches(&Permission::ManageRoles));
        assert!(Permission::ManageRoles.matches(&Permission::ManageRoles));
        assert!(Permission::ManageUsers.matches(&Permission::ManageUsers));

        // Create
        assert!(
            Permission::CreateDocument(None)
                .matches(&Permission::CreateDocument(Some(doc_type.clone())))
        );
        assert!(
            Permission::CreateDocument(Some(doc_type.clone()))
                .matches(&Permission::CreateDocument(Some(doc_type.clone())))
        );
        assert!(
            !Permission::CreateDocument(Some(doc_type.clone()))
                .matches(&Permission::CreateDocument(Some(other_type.clone())))
        );

        // Update
        assert!(
            Permission::UpdateDocument(None)
                .matches(&Permission::UpdateDocument(Some(doc_type.clone())))
        );
        assert!(
            Permission::UpdateDocument(Some(doc_type.clone()))
                .matches(&Permission::UpdateDocument(Some(doc_type.clone())))
        );
        assert!(
            !Permission::UpdateDocument(Some(doc_type.clone()))
                .matches(&Permission::UpdateDocument(Some(other_type.clone())))
        );

        // Delete
        assert!(
            Permission::DeleteDocument(None)
                .matches(&Permission::DeleteDocument(Some(doc_type.clone())))
        );
        assert!(
            Permission::DeleteDocument(Some(doc_type.clone()))
                .matches(&Permission::DeleteDocument(Some(doc_type.clone())))
        );
        assert!(
            !Permission::DeleteDocument(Some(doc_type.clone()))
                .matches(&Permission::DeleteDocument(Some(other_type.clone())))
        );

        // Publish
        assert!(
            Permission::PublishDocument(None)
                .matches(&Permission::PublishDocument(Some(doc_type.clone())))
        );
        assert!(
            Permission::PublishDocument(Some(doc_type.clone()))
                .matches(&Permission::PublishDocument(Some(doc_type.clone())))
        );
        assert!(
            !Permission::PublishDocument(Some(doc_type.clone()))
                .matches(&Permission::PublishDocument(Some(other_type)))
        );
    }

    #[test]
    fn test_role_has_permission() {
        let doc_type = create_doc_type_id("article");
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "editor".into(),
            description: None,
            permissions: vec![
                Permission::ReadDocument(None),
                Permission::UpdateDocument(Some(doc_type.clone())),
            ],
        };

        // Granted via wildcard
        assert!(role.has_permission(&Permission::ReadDocument(Some(doc_type.clone()))));
        // Granted via exact match
        assert!(role.has_permission(&Permission::UpdateDocument(Some(doc_type.clone()))));
        // Denied - action not in permissions
        assert!(!role.has_permission(&Permission::DeleteDocument(Some(doc_type))));
        assert!(!role.has_permission(&Permission::ManageSchema));
    }
}
