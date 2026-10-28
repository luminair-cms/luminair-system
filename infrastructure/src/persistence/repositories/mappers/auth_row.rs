//! Row mappers for `Role`, `Permission`, `UserRoleAssignment`, `AccessRequest`, and `UserProfile`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use domain::auth::access_request::{AccessRequest, AccessRequestId};
use domain::auth::profile::UserProfile;
use domain::auth::role::{Permission, Role, RoleId, UserRoleAssignment, UserRoleAssignmentId};
use domain::auth::user::UserId;
use domain::common::{DisplayName, Email};

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{
    COL_ACTION, COL_ASSIGNED_ROLES, COL_DESCRIPTION, COL_DOCUMENT_TYPE_ID, COL_EMAIL,
    COL_FIRST_SEEN_AT, COL_GRANTED_AT, COL_GRANTED_BY, COL_ID, COL_LAST_SEEN_AT, COL_NAME,
    COL_REJECTION_REASON, COL_REQUESTED_AT, COL_REVIEWED_AT, COL_REVIEWED_BY, COL_ROLE_ID,
    COL_STATUS, COL_USER_ID,
};
use crate::persistence::repositories::codec::{
    access_request_status_from_db, deserialize_assigned_roles_json, parse_optional_actor_urn,
    permission_from_db_row,
};

// ---------------------------------------------------------------------------
// Public Functions
// ---------------------------------------------------------------------------

/// Maps a row from `roles` into a base tuple of `(RoleId, name, description)`.
pub fn map_role_base_row(row: &PgRow) -> Result<(RoleId, String, Option<String>), PersistenceError> {
    let id: Uuid = row.try_get(COL_ID)?;
    let name: String = row.try_get(COL_NAME)?;
    let description: Option<String> = row.try_get(COL_DESCRIPTION)?;
    Ok((RoleId::new(id), name, description))
}

/// Maps a row from `role_permissions` into a `(RoleId, Permission)` pair.
pub fn map_permission_row(row: &PgRow) -> Result<(RoleId, Permission), PersistenceError> {
    let role_id: Uuid = row.try_get(COL_ROLE_ID)?;
    let action: &str = row.try_get(COL_ACTION)?;
    let doc_type_id: Option<String> = row.try_get(COL_DOCUMENT_TYPE_ID)?;
    let perm = permission_from_db_row(action, doc_type_id)?;
    Ok((RoleId::new(role_id), perm))
}

/// Assembles complete [`Role`] entities by pairing base role records with their permissions.
#[must_use]
pub fn assemble_roles(
    base_rows: Vec<(RoleId, String, Option<String>)>,
    perm_rows: Vec<(RoleId, Permission)>,
) -> Vec<Role> {
    let mut perm_map: HashMap<RoleId, Vec<Permission>> = HashMap::new();
    for (role_id, perm) in perm_rows {
        perm_map.entry(role_id).or_default().push(perm);
    }

    base_rows
        .into_iter()
        .map(|(id, name, description)| {
            let permissions = perm_map.remove(&id).unwrap_or_default();
            Role {
                id,
                name,
                description,
                permissions,
            }
        })
        .collect()
}

/// Maps a row from `user_role_assignments` into a [`UserRoleAssignment`].
pub fn map_user_role_assignment_row(row: &PgRow) -> Result<UserRoleAssignment, PersistenceError> {
    let id: Uuid = row.try_get(COL_ID)?;
    let user_id_str: String = row.try_get(COL_USER_ID)?;
    let user_id = UserId::try_new(user_id_str)
        .map_err(|e| PersistenceError::Validation(e.to_string()))?;
    let role_id: Uuid = row.try_get(COL_ROLE_ID)?;
    let granted_at: DateTime<Utc> = row.try_get(COL_GRANTED_AT)?;
    let granted_by_str: Option<&str> = row.try_get(COL_GRANTED_BY)?;
    let granted_by = parse_optional_actor_urn(granted_by_str)?;

    Ok(UserRoleAssignment {
        id: UserRoleAssignmentId::new(id),
        user_id,
        role_id: RoleId::new(role_id),
        granted_at,
        granted_by,
    })
}

/// Maps a row from `access_requests` into an [`AccessRequest`].
pub fn map_access_request_row(row: &PgRow) -> Result<AccessRequest, PersistenceError> {
    let id: Uuid = row.try_get(COL_ID)?;
    let user_id_str: String = row.try_get(COL_USER_ID)?;
    let user_id = UserId::try_new(user_id_str)
        .map_err(|e| PersistenceError::Validation(e.to_string()))?;
    let email_str: Option<String> = row.try_get(COL_EMAIL)?;
    let email = email_str
        .map(|s| Email::try_new(s).map_err(|e| PersistenceError::Validation(e.to_string())))
        .transpose()?;
    let name_str: Option<String> = row.try_get(COL_NAME)?;
    let name = name_str
        .map(|s| DisplayName::try_new(s).map_err(|e| PersistenceError::Validation(e.to_string())))
        .transpose()?;
    let requested_at: DateTime<Utc> = row.try_get(COL_REQUESTED_AT)?;
    let status_str: &str = row.try_get(COL_STATUS)?;
    let rejection_reason: Option<String> = row.try_get(COL_REJECTION_REASON)?;
    let status = access_request_status_from_db(status_str, rejection_reason)?;
    let reviewed_by_str: Option<&str> = row.try_get(COL_REVIEWED_BY)?;
    let reviewed_by = parse_optional_actor_urn(reviewed_by_str)?;
    let reviewed_at: Option<DateTime<Utc>> = row.try_get(COL_REVIEWED_AT)?;
    let assigned_roles_val: serde_json::Value = row.try_get(COL_ASSIGNED_ROLES)?;
    let assigned_roles = deserialize_assigned_roles_json(assigned_roles_val)?;

    Ok(AccessRequest {
        id: AccessRequestId::new(id),
        user_id,
        email,
        name,
        requested_at,
        status,
        reviewed_by,
        reviewed_at,
        assigned_roles,
    })
}

/// Maps a row from `user_profiles` into a [`UserProfile`] entity.
pub fn map_user_profile_row(row: &PgRow) -> Result<UserProfile, PersistenceError> {
    let user_id_str: String = row.try_get(COL_USER_ID)?;
    let user_id = UserId::try_new(user_id_str)
        .map_err(|e| PersistenceError::Validation(e.to_string()))?;
    let email_str: Option<String> = row.try_get(COL_EMAIL)?;
    let email = email_str
        .map(|s| Email::try_new(s).map_err(|e| PersistenceError::Validation(e.to_string())))
        .transpose()?;
    let name_str: Option<String> = row.try_get(COL_NAME)?;
    let name = name_str
        .map(|s| DisplayName::try_new(s).map_err(|e| PersistenceError::Validation(e.to_string())))
        .transpose()?;
    let first_seen_at: DateTime<Utc> = row.try_get(COL_FIRST_SEEN_AT)?;
    let last_seen_at: DateTime<Utc> = row.try_get(COL_LAST_SEEN_AT)?;

    Ok(UserProfile::reconstitute(
        user_id,
        email,
        name,
        first_seen_at,
        last_seen_at,
    ))
}

/// Maps multiple `user_profiles` rows into a `Vec<UserProfile>`.
pub fn map_user_profile_rows(rows: &[PgRow]) -> Result<Vec<UserProfile>, PersistenceError> {
    rows.iter().map(map_user_profile_row).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemble_roles() {
        let r1 = RoleId::new(Uuid::now_v7());
        let r2 = RoleId::new(Uuid::now_v7());

        let base_rows = vec![
            (r1, "Admin".into(), Some("Administrator role".into())),
            (r2, "Editor".into(), None),
        ];

        let perm_rows = vec![
            (r1, Permission::ManageSchema),
            (r1, Permission::ManageRoles),
            (r2, Permission::CreateDocument(None)),
        ];

        let roles = assemble_roles(base_rows, perm_rows);
        assert_eq!(roles.len(), 2);
        assert_eq!(roles[0].id, r1);
        assert_eq!(roles[0].name, "Admin");
        assert_eq!(roles[0].permissions.len(), 2);
        assert_eq!(roles[1].id, r2);
        assert_eq!(roles[1].name, "Editor");
        assert_eq!(roles[1].permissions.len(), 1);
    }
}
