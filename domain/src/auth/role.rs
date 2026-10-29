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

/// Zero-allocation borrowed representation of an action permission check.
///
/// Holds references to [`DocumentTypeId`] to avoid cloning across the authorization hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionCheck<'a> {
    ManageSchema,
    CreateDocument(Option<&'a DocumentTypeId>),
    ReadDocument(Option<&'a DocumentTypeId>),
    UpdateDocument(Option<&'a DocumentTypeId>),
    DeleteDocument(Option<&'a DocumentTypeId>),
    PublishDocument(Option<&'a DocumentTypeId>),
    ManageRoles,
    ManageUsers,
}

impl<'a> PermissionCheck<'a> {
    #[must_use]
    pub const fn create_document(type_id: &'a DocumentTypeId) -> Self {
        Self::CreateDocument(Some(type_id))
    }

    #[must_use]
    pub const fn read_document(type_id: &'a DocumentTypeId) -> Self {
        Self::ReadDocument(Some(type_id))
    }

    #[must_use]
    pub const fn update_document(type_id: &'a DocumentTypeId) -> Self {
        Self::UpdateDocument(Some(type_id))
    }

    #[must_use]
    pub const fn delete_document(type_id: &'a DocumentTypeId) -> Self {
        Self::DeleteDocument(Some(type_id))
    }

    #[must_use]
    pub const fn publish_document(type_id: &'a DocumentTypeId) -> Self {
        Self::PublishDocument(Some(type_id))
    }

    /// Converts this borrowed permission check into an owned [`Permission`], cloning any borrowed identifiers.
    ///
    /// Primarily used when constructing detailed unauthorized errors on the error path.
    #[must_use]
    pub fn to_permission(&self) -> Permission {
        match *self {
            Self::ManageSchema => Permission::ManageSchema,
            Self::CreateDocument(opt) => Permission::CreateDocument(opt.cloned()),
            Self::ReadDocument(opt) => Permission::ReadDocument(opt.cloned()),
            Self::UpdateDocument(opt) => Permission::UpdateDocument(opt.cloned()),
            Self::DeleteDocument(opt) => Permission::DeleteDocument(opt.cloned()),
            Self::PublishDocument(opt) => Permission::PublishDocument(opt.cloned()),
            Self::ManageRoles => Permission::ManageRoles,
            Self::ManageUsers => Permission::ManageUsers,
        }
    }
}

impl<'a> From<&'a Permission> for PermissionCheck<'a> {
    fn from(p: &'a Permission) -> Self {
        match p {
            Permission::ManageSchema => Self::ManageSchema,
            Permission::CreateDocument(opt) => Self::CreateDocument(opt.as_ref()),
            Permission::ReadDocument(opt) => Self::ReadDocument(opt.as_ref()),
            Permission::UpdateDocument(opt) => Self::UpdateDocument(opt.as_ref()),
            Permission::DeleteDocument(opt) => Self::DeleteDocument(opt.as_ref()),
            Permission::PublishDocument(opt) => Self::PublishDocument(opt.as_ref()),
            Permission::ManageRoles => Self::ManageRoles,
            Permission::ManageUsers => Self::ManageUsers,
        }
    }
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
    pub fn has_permission(&self, action: &PermissionCheck<'_>) -> bool {
        self.permissions.iter().any(|p| p.matches(action))
    }
}

impl Permission {
    /// Checks if this permission satisfies the requested action check.
    /// `None` indicates a wildcard granting access to all document types.
    #[must_use]
    pub fn matches(&self, action: &PermissionCheck<'_>) -> bool {
        match (self, action) {
            (Permission::ManageSchema, PermissionCheck::ManageSchema)
            | (Permission::ManageRoles, PermissionCheck::ManageRoles)
            | (Permission::ManageUsers, PermissionCheck::ManageUsers)
            | (Permission::CreateDocument(None), PermissionCheck::CreateDocument(_))
            | (Permission::ReadDocument(None), PermissionCheck::ReadDocument(_))
            | (Permission::UpdateDocument(None), PermissionCheck::UpdateDocument(_))
            | (Permission::DeleteDocument(None), PermissionCheck::DeleteDocument(_))
            | (Permission::PublishDocument(None), PermissionCheck::PublishDocument(_)) => true,
            (Permission::CreateDocument(Some(a)), PermissionCheck::CreateDocument(Some(b)))
            | (Permission::ReadDocument(Some(a)), PermissionCheck::ReadDocument(Some(b)))
            | (Permission::UpdateDocument(Some(a)), PermissionCheck::UpdateDocument(Some(b)))
            | (Permission::DeleteDocument(Some(a)), PermissionCheck::DeleteDocument(Some(b)))
            | (Permission::PublishDocument(Some(a)), PermissionCheck::PublishDocument(Some(b))) => {
                a == *b
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
        let exact_a = Permission::ReadDocument(Some(type_a.clone()));

        // Wildcard matches any type
        assert!(wildcard.matches(&PermissionCheck::read_document(&type_a)));
        assert!(wildcard.matches(&PermissionCheck::read_document(&type_b)));

        // Exact matches only same type
        assert!(exact_a.matches(&PermissionCheck::read_document(&type_a)));
        assert!(!exact_a.matches(&PermissionCheck::read_document(&type_b)));
        assert!(!exact_a.matches(&PermissionCheck::ReadDocument(None)));

        // Different actions do not match
        assert!(!Permission::ManageSchema.matches(&PermissionCheck::read_document(&type_a)));
        assert!(
            !Permission::CreateDocument(None).matches(&PermissionCheck::read_document(&type_a))
        );

        assert_eq!(
            PermissionCheck::read_document(&type_a).to_permission(),
            exact_a
        );
    }

    #[test]
    fn test_permission_matches_all_variants_wildcards_and_exact() {
        let doc_type = create_doc_type_id("article");
        let other_type = create_doc_type_id("author");

        // System-level permissions
        assert!(Permission::ManageSchema.matches(&PermissionCheck::ManageSchema));
        assert!(!Permission::ManageSchema.matches(&PermissionCheck::ManageRoles));
        assert!(Permission::ManageRoles.matches(&PermissionCheck::ManageRoles));
        assert!(Permission::ManageUsers.matches(&PermissionCheck::ManageUsers));

        // Create
        assert!(
            Permission::CreateDocument(None).matches(&PermissionCheck::create_document(&doc_type))
        );
        assert!(
            Permission::CreateDocument(Some(doc_type.clone()))
                .matches(&PermissionCheck::create_document(&doc_type))
        );
        assert!(
            !Permission::CreateDocument(Some(doc_type.clone()))
                .matches(&PermissionCheck::create_document(&other_type))
        );

        // Update
        assert!(
            Permission::UpdateDocument(None).matches(&PermissionCheck::update_document(&doc_type))
        );
        assert!(
            Permission::UpdateDocument(Some(doc_type.clone()))
                .matches(&PermissionCheck::update_document(&doc_type))
        );
        assert!(
            !Permission::UpdateDocument(Some(doc_type.clone()))
                .matches(&PermissionCheck::update_document(&other_type))
        );

        // Delete
        assert!(
            Permission::DeleteDocument(None).matches(&PermissionCheck::delete_document(&doc_type))
        );
        assert!(
            Permission::DeleteDocument(Some(doc_type.clone()))
                .matches(&PermissionCheck::delete_document(&doc_type))
        );
        assert!(
            !Permission::DeleteDocument(Some(doc_type.clone()))
                .matches(&PermissionCheck::delete_document(&other_type))
        );

        // Publish
        assert!(
            Permission::PublishDocument(None)
                .matches(&PermissionCheck::publish_document(&doc_type))
        );
        assert!(
            Permission::PublishDocument(Some(doc_type.clone()))
                .matches(&PermissionCheck::publish_document(&doc_type))
        );
        assert!(
            !Permission::PublishDocument(Some(doc_type.clone()))
                .matches(&PermissionCheck::publish_document(&other_type))
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
        assert!(role.has_permission(&PermissionCheck::read_document(&doc_type)));
        // Granted via exact match
        assert!(role.has_permission(&PermissionCheck::update_document(&doc_type)));
        // Denied - action not in permissions
        assert!(!role.has_permission(&PermissionCheck::delete_document(&doc_type)));
        assert!(!role.has_permission(&PermissionCheck::ManageSchema));
    }
}
