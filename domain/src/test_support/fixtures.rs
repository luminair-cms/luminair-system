use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::ids::{test_doc_type_id, test_locales, test_user_id};
use crate::auth::access_request::AccessRequest;
use crate::auth::role::{Permission, Role, RoleId};
use crate::auth::user::UserId;
use crate::common::{DisplayName, Email};
use crate::content::instance::{DocumentContent, DocumentInstance, PublicationState};
use crate::content::values::ContentValue;
use crate::schema::attributes::AttributeId;
use crate::system::config::{SystemConfig, SystemConfigId};

/// Returns a default `SystemConfig` configured with "en" (default) and "uk".
pub fn fixture_system_config() -> SystemConfig {
    let (en, uk, _) = test_locales();
    SystemConfig::new(
        SystemConfigId::new(Uuid::now_v7()),
        vec![en.clone(), uk],
        en,
    )
    .expect("valid system config fixture")
}

/// Returns a new `DocumentInstance` in Draft state with audit initialized.
pub fn fixture_document_instance(type_id: &str, owner_name: Option<&str>) -> DocumentInstance {
    let doc_type = test_doc_type_id(type_id);
    let owner = owner_name.map(test_user_id);
    DocumentInstance::new(doc_type, owner, Utc::now())
}

/// Returns a pre-built `AccessRequest` in Pending state.
pub fn fixture_access_request(user_name: &str) -> (AccessRequest, UserId, DateTime<Utc>) {
    let user = test_user_id(user_name);
    let email = Email::try_new("test@example.com").ok();
    let name = DisplayName::try_new("Test User").ok();
    let now = Utc::now();
    let req = AccessRequest::new(user.clone(), email, name, now);
    (req, user, now)
}

/// Returns an admin role possessing all system and wildcard document permissions.
pub fn admin_role() -> Role {
    Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "admin".into(),
        description: Some("Full system administrator".into()),
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
    }
}

/// Shorthand to package a map of fields into `DocumentContent` in Draft state.
pub fn draft_content(fields: HashMap<AttributeId, ContentValue>) -> DocumentContent {
    DocumentContent {
        fields,
        publication_state: PublicationState::Draft {
            last_published_revision: None,
        },
    }
}
