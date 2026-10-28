//! Sea-Query dynamic query builders for RBAC (`roles`, `role_permissions`, `user_role_assignments`), `access_requests`, and `user_profiles`.

use sea_query::{
    Condition, DeleteStatement, Expr, ExprTrait, Func, InsertStatement, OnConflict, Order, Query,
    SelectStatement,
};
use uuid::Uuid;

use domain::auth::access_request::AccessRequest;
use domain::auth::profile::UserProfile;
use domain::auth::role::{Role, UserRoleAssignment};
use domain::auth::user::UserId;
use domain::common::Pagination;

use crate::persistence::naming::StaticIden;
use super::common::{
    apply_pagination, COL_ACTION_IDEN, COL_ASSIGNED_ROLES_IDEN, COL_CREATED_AT_IDEN,
    COL_DESCRIPTION_IDEN, COL_DOCUMENT_TYPE_ID_IDEN, COL_EMAIL_IDEN, COL_FIRST_SEEN_AT_IDEN,
    COL_GRANTED_AT_IDEN, COL_GRANTED_BY_IDEN, COL_ID_IDEN, COL_LAST_SEEN_AT_IDEN,
    COL_NAME_IDEN, COL_REJECTION_REASON_IDEN, COL_REQUESTED_AT_IDEN, COL_REVIEWED_AT_IDEN,
    COL_REVIEWED_BY_IDEN, COL_ROLE_ID_IDEN, COL_STATUS_IDEN, COL_UPDATED_AT_IDEN,
    COL_USER_ID_IDEN, TABLE_ACCESS_REQUESTS_IDEN, TABLE_ROLE_PERMISSIONS_IDEN, TABLE_ROLES_IDEN,
    TABLE_USER_PROFILES_IDEN, TABLE_USER_ROLE_ASSIGNMENTS_IDEN,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ROLE_COLUMNS: [StaticIden; 3] = [COL_ID_IDEN, COL_NAME_IDEN, COL_DESCRIPTION_IDEN];

const PERMISSION_COLUMNS: [StaticIden; 3] = [
    COL_ROLE_ID_IDEN,
    COL_ACTION_IDEN,
    COL_DOCUMENT_TYPE_ID_IDEN,
];

const PERMISSION_INSERT_COLUMNS: [StaticIden; 4] = [
    COL_ID_IDEN,
    COL_ROLE_ID_IDEN,
    COL_ACTION_IDEN,
    COL_DOCUMENT_TYPE_ID_IDEN,
];

const USER_ROLE_ASSIGNMENT_COLUMNS: [StaticIden; 5] = [
    COL_ID_IDEN,
    COL_USER_ID_IDEN,
    COL_ROLE_ID_IDEN,
    COL_GRANTED_AT_IDEN,
    COL_GRANTED_BY_IDEN,
];

const ACCESS_REQUEST_COLUMNS: [StaticIden; 10] = [
    COL_ID_IDEN,
    COL_USER_ID_IDEN,
    COL_EMAIL_IDEN,
    COL_NAME_IDEN,
    COL_REQUESTED_AT_IDEN,
    COL_STATUS_IDEN,
    COL_REJECTION_REASON_IDEN,
    COL_REVIEWED_BY_IDEN,
    COL_REVIEWED_AT_IDEN,
    COL_ASSIGNED_ROLES_IDEN,
];

const USER_PROFILE_COLUMNS: [StaticIden; 5] = [
    COL_USER_ID_IDEN,
    COL_EMAIL_IDEN,
    COL_NAME_IDEN,
    COL_FIRST_SEEN_AT_IDEN,
    COL_LAST_SEEN_AT_IDEN,
];

// ---------------------------------------------------------------------------
// Roles & Permissions Query Builders
// ---------------------------------------------------------------------------

/// Builds a `SELECT id, name, description FROM roles WHERE id = $1` query.
#[must_use]
pub fn select_role_by_id_query(id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLES_IDEN)
        .columns(ROLE_COLUMNS)
        .and_where(Expr::col(COL_ID_IDEN).eq(id));
    stmt
}

/// Builds a `SELECT id, name, description FROM roles WHERE id IN (...) ORDER BY name ASC` query.
#[must_use]
pub fn select_roles_by_ids_query(ids: &[Uuid]) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLES_IDEN)
        .columns(ROLE_COLUMNS)
        .and_where(Expr::col(COL_ID_IDEN).is_in(ids.to_vec()))
        .order_by(COL_NAME_IDEN, Order::Asc);
    stmt
}

/// Builds a `SELECT id, name, description FROM roles WHERE name = $1` query.
#[must_use]
pub fn select_role_by_name_query(name: &str) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLES_IDEN)
        .columns(ROLE_COLUMNS)
        .and_where(Expr::col(COL_NAME_IDEN).eq(name));
    stmt
}

/// Builds a `SELECT id, name, description FROM roles ORDER BY name ASC` query.
#[must_use]
pub fn select_all_roles_query() -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLES_IDEN)
        .columns(ROLE_COLUMNS)
        .order_by(COL_NAME_IDEN, Order::Asc);
    stmt
}

/// Builds a query to fetch all permissions for a set of role IDs ordered by creation time.
#[must_use]
pub fn select_permissions_by_role_ids_query(role_ids: &[Uuid]) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLE_PERMISSIONS_IDEN)
        .columns(PERMISSION_COLUMNS)
        .and_where(Expr::col(COL_ROLE_ID_IDEN).is_in(role_ids.to_vec()))
        .order_by(COL_CREATED_AT_IDEN, Order::Asc)
        .order_by(COL_ID_IDEN, Order::Asc);
    stmt
}

/// Builds an upsert statement for a `Role` with `ON CONFLICT (id) DO UPDATE`.
#[must_use]
pub fn upsert_role_query(role: &Role) -> InsertStatement {
    let mut stmt = Query::insert();
    stmt.into_table(TABLE_ROLES_IDEN)
        .columns(ROLE_COLUMNS)
        .values_panic([
            (*role.id).into(),
            role.name.clone().into(),
            role.description.clone().into(),
        ])
        .on_conflict(
            OnConflict::column(COL_ID_IDEN)
                .update_columns([COL_NAME_IDEN, COL_DESCRIPTION_IDEN])
                .value(COL_UPDATED_AT_IDEN, Expr::current_timestamp())
                .to_owned(),
        );
    stmt
}

/// Builds a `DELETE FROM role_permissions WHERE role_id = $1` statement.
#[must_use]
pub fn delete_permissions_by_role_query(role_id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(TABLE_ROLE_PERMISSIONS_IDEN)
        .and_where(Expr::col(COL_ROLE_ID_IDEN).eq(role_id));
    stmt
}

/// Builds a batch `INSERT INTO role_permissions` statement, or `None` if `permissions` is empty.
#[must_use]
pub fn insert_permissions_batch_query(
    role_id: Uuid,
    permissions: &[(&str, Option<&str>)],
) -> Option<InsertStatement> {
    if permissions.is_empty() {
        return None;
    }

    let mut stmt = Query::insert();
    stmt.into_table(TABLE_ROLE_PERMISSIONS_IDEN)
        .columns(PERMISSION_INSERT_COLUMNS);

    for (action, doc_type_id) in permissions {
        stmt.values_panic([
            Uuid::now_v7().into(),
            role_id.into(),
            (*action).into(),
            (*doc_type_id).into(),
        ]);
    }

    Some(stmt)
}

// ---------------------------------------------------------------------------
// User Role Assignments Query Builders
// ---------------------------------------------------------------------------

/// Builds a query to fetch all role assignments for a given user.
#[must_use]
pub fn select_assignments_by_user_query(user_id: &UserId) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_USER_ROLE_ASSIGNMENTS_IDEN)
        .columns(USER_ROLE_ASSIGNMENT_COLUMNS)
        .and_where(Expr::col(COL_USER_ID_IDEN).eq(user_id.as_ref()))
        .order_by(COL_GRANTED_AT_IDEN, Order::Asc)
        .order_by(COL_ID_IDEN, Order::Asc);
    stmt
}

/// Builds an existence check for any user assignment with the given admin role ID.
#[must_use]
pub fn exists_admin_assignment_query(admin_role_id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_USER_ROLE_ASSIGNMENTS_IDEN)
        .expr(Expr::val(1))
        .and_where(Expr::col(COL_ROLE_ID_IDEN).eq(admin_role_id))
        .limit(1);
    stmt
}

/// Builds an upsert statement for a `UserRoleAssignment` with `ON CONFLICT (user_id, role_id) DO UPDATE`.
#[must_use]
pub fn upsert_assignment_query(assignment: &UserRoleAssignment) -> InsertStatement {
    let granted_by_urn = assignment.granted_by.as_ref().map(ToString::to_string);
    let mut stmt = Query::insert();
    stmt.into_table(TABLE_USER_ROLE_ASSIGNMENTS_IDEN)
        .columns(USER_ROLE_ASSIGNMENT_COLUMNS)
        .values_panic([
            (*assignment.id).into(),
            assignment.user_id.as_ref().into(),
            (*assignment.role_id).into(),
            assignment.granted_at.into(),
            granted_by_urn.into(),
        ])
        .on_conflict(
            OnConflict::columns([COL_USER_ID_IDEN, COL_ROLE_ID_IDEN])
                .update_columns([COL_GRANTED_AT_IDEN, COL_GRANTED_BY_IDEN])
                .to_owned(),
        );
    stmt
}

/// Builds a `DELETE FROM user_role_assignments WHERE id = $1` statement.
#[must_use]
pub fn delete_assignment_query(id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(TABLE_USER_ROLE_ASSIGNMENTS_IDEN)
        .and_where(Expr::col(COL_ID_IDEN).eq(id));
    stmt
}

// ---------------------------------------------------------------------------
// Access Requests Query Builders
// ---------------------------------------------------------------------------

/// Builds a query to fetch an access request by primary key.
#[must_use]
pub fn select_access_request_by_id_query(id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ACCESS_REQUESTS_IDEN)
        .columns(ACCESS_REQUEST_COLUMNS)
        .and_where(Expr::col(COL_ID_IDEN).eq(id));
    stmt
}

/// Builds a query to fetch the latest access request submitted by a user.
#[must_use]
pub fn select_latest_access_request_by_user_query(user_id: &UserId) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ACCESS_REQUESTS_IDEN)
        .columns(ACCESS_REQUEST_COLUMNS)
        .and_where(Expr::col(COL_USER_ID_IDEN).eq(user_id.as_ref()))
        .order_by(COL_REQUESTED_AT_IDEN, Order::Desc)
        .limit(1);
    stmt
}

/// Builds a query to fetch an active (`pending` or `approved`) access request for a user.
#[must_use]
pub fn select_active_access_request_by_user_query(user_id: &UserId) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ACCESS_REQUESTS_IDEN)
        .columns(ACCESS_REQUEST_COLUMNS)
        .and_where(Expr::col(COL_USER_ID_IDEN).eq(user_id.as_ref()))
        .and_where(Expr::col(COL_STATUS_IDEN).is_in(["pending", "approved"]))
        .order_by(COL_REQUESTED_AT_IDEN, Order::Desc)
        .limit(1);
    stmt
}

/// Builds a query to fetch all pending access requests ordered by request time ascending.
#[must_use]
pub fn select_pending_access_requests_query() -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ACCESS_REQUESTS_IDEN)
        .columns(ACCESS_REQUEST_COLUMNS)
        .and_where(Expr::col(COL_STATUS_IDEN).eq("pending"))
        .order_by(COL_REQUESTED_AT_IDEN, Order::Asc);
    stmt
}

/// Builds an upsert statement for an `AccessRequest` with `ON CONFLICT (id) DO UPDATE`.
#[must_use]
pub fn upsert_access_request_query(
    request: &AccessRequest,
    status_str: &'static str,
    rejection_reason: Option<&str>,
    assigned_roles_json: serde_json::Value,
) -> InsertStatement {
    let reviewed_by_urn = request.reviewed_by.as_ref().map(ToString::to_string);

    let mut stmt = Query::insert();
    stmt.into_table(TABLE_ACCESS_REQUESTS_IDEN)
        .columns(ACCESS_REQUEST_COLUMNS)
        .values_panic([
            (*request.id).into(),
            request.user_id.as_ref().into(),
            request.email.as_ref().map(AsRef::as_ref).into(),
            request.name.as_ref().map(AsRef::as_ref).into(),
            request.requested_at.into(),
            status_str.into(),
            rejection_reason.into(),
            reviewed_by_urn.into(),
            request.reviewed_at.into(),
            assigned_roles_json.into(),
        ])
        .on_conflict(
            OnConflict::column(COL_ID_IDEN)
                .update_columns([
                    COL_STATUS_IDEN,
                    COL_REJECTION_REASON_IDEN,
                    COL_REVIEWED_BY_IDEN,
                    COL_REVIEWED_AT_IDEN,
                    COL_ASSIGNED_ROLES_IDEN,
                ])
                .to_owned(),
        );
    stmt
}

// ---------------------------------------------------------------------------
// User Profiles Query Builders
// ---------------------------------------------------------------------------

/// Builds a query to fetch a single `UserProfile` by `user_id`.
#[must_use]
pub fn select_user_profile_by_id_query(user_id: &UserId) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_USER_PROFILES_IDEN)
        .columns(USER_PROFILE_COLUMNS)
        .and_where(Expr::col(COL_USER_ID_IDEN).eq(user_id.as_ref()));
    stmt
}

/// Builds a query to fetch multiple `UserProfile` records by a list of `user_ids`.
#[must_use]
pub fn select_user_profiles_by_ids_query(user_ids: &[UserId]) -> SelectStatement {
    let ids: Vec<&str> = user_ids.iter().map(AsRef::as_ref).collect();
    let mut stmt = Query::select();
    stmt.from(TABLE_USER_PROFILES_IDEN)
        .columns(USER_PROFILE_COLUMNS)
        .and_where(Expr::col(COL_USER_ID_IDEN).is_in(ids));
    stmt
}

/// Builds a paginated query to list `UserProfile` records matching an optional search string.
#[must_use]
pub fn select_user_profiles_paginated_query(
    query: Option<&str>,
    pagination: Pagination,
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_USER_PROFILES_IDEN)
        .columns(USER_PROFILE_COLUMNS);

    apply_user_profile_search_filter(&mut stmt, query);

    stmt.order_by(COL_USER_ID_IDEN, Order::Asc);
    apply_pagination(&mut stmt, pagination);
    stmt
}

/// Builds a count query for `UserProfile` records matching an optional search string.
#[must_use]
pub fn count_user_profiles_query(query: Option<&str>) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_USER_PROFILES_IDEN)
        .expr(Func::count(Expr::col(COL_USER_ID_IDEN)));

    apply_user_profile_search_filter(&mut stmt, query);
    stmt
}

fn apply_user_profile_search_filter(stmt: &mut SelectStatement, query: Option<&str>) {
    use sea_query::extension::postgres::PgExpr;

    if let Some(q) = query {
        let trimmed = q.trim();
        if !trimmed.is_empty() {
            let pattern = format!("%{trimmed}%");
            stmt.cond_where(
                Condition::any()
                    .add(Expr::col(COL_EMAIL_IDEN).ilike(pattern.clone()))
                    .add(Expr::col(COL_NAME_IDEN).ilike(pattern.clone()))
                    .add(Expr::col(COL_USER_ID_IDEN).ilike(pattern)),
            );
        }
    }
}

/// Builds an upsert statement for a `UserProfile`.
///
/// On conflict on `user_id`, updates `email`, `name`, and `last_seen_at` while preserving `first_seen_at`.
#[must_use]
pub fn upsert_user_profile_query(profile: &UserProfile) -> InsertStatement {
    let mut stmt = Query::insert();
    stmt.into_table(TABLE_USER_PROFILES_IDEN)
        .columns(USER_PROFILE_COLUMNS)
        .values_panic([
            profile.user_id.as_ref().into(),
            profile.email.as_ref().map(AsRef::as_ref).into(),
            profile.name.as_ref().map(AsRef::as_ref).into(),
            profile.first_seen_at.into(),
            profile.last_seen_at.into(),
        ])
        .on_conflict(
            OnConflict::column(COL_USER_ID_IDEN)
                .update_columns([COL_EMAIL_IDEN, COL_NAME_IDEN, COL_LAST_SEEN_AT_IDEN])
                .to_owned(),
        );
    stmt
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sea_query::PostgresQueryBuilder;
    use serde_json::json;

    use domain::auth::access_request::{AccessRequestId, AccessRequestStatus};
    use domain::auth::role::{RoleId, UserRoleAssignmentId};
    use domain::auth::user::ActorId;
    use domain::common::{DisplayName, Email};
    use crate::persistence::query::common::{compile_delete, compile_insert, compile_select};

    #[test]
    fn test_role_queries() {
        let role_id = Uuid::now_v7();
        let stmt_by_id = select_role_by_id_query(role_id);
        let unparam_id = stmt_by_id.to_string(PostgresQueryBuilder);
        assert!(unparam_id.contains("FROM \"roles\" WHERE \"id\" ="));

        let stmt_by_ids = select_roles_by_ids_query(&[role_id]);
        let unparam_ids = stmt_by_ids.to_string(PostgresQueryBuilder);
        assert!(unparam_ids.contains("FROM \"roles\" WHERE \"id\" IN"));
        assert!(unparam_ids.contains("ORDER BY \"name\" ASC"));

        let stmt_by_name = select_role_by_name_query("Editor");
        let unparam_name = stmt_by_name.to_string(PostgresQueryBuilder);
        assert!(unparam_name.contains("FROM \"roles\" WHERE \"name\" = 'Editor'"));

        let stmt_all = select_all_roles_query();
        let unparam_all = stmt_all.to_string(PostgresQueryBuilder);
        assert!(unparam_all.contains("FROM \"roles\" ORDER BY \"name\" ASC"));
    }

    #[test]
    fn test_upsert_role_query() {
        let role = Role {
            id: RoleId::new(Uuid::now_v7()),
            name: "Admin".to_string(),
            description: Some("System Administrator".to_string()),
            permissions: vec![],
        };
        let stmt = upsert_role_query(&role);
        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("INSERT INTO \"roles\""));
        assert!(unparam.contains("ON CONFLICT (\"id\") DO UPDATE SET"));
        assert!(unparam.contains("\"name\" = \"excluded\".\"name\""));
        assert!(unparam.contains("\"updated_at\" = CURRENT_TIMESTAMP"));

        let (sql, _) = compile_insert(&stmt);
        assert!(sql.contains("INSERT INTO \"roles\""));
    }

    #[test]
    fn test_permissions_queries() {
        let role_id = Uuid::now_v7();
        let stmt_perms = select_permissions_by_role_ids_query(&[role_id]);
        let unparam_perms = stmt_perms.to_string(PostgresQueryBuilder);
        assert!(unparam_perms.contains("FROM \"role_permissions\""));
        assert!(unparam_perms.contains("ORDER BY \"created_at\" ASC, \"id\" ASC"));

        let stmt_del = delete_permissions_by_role_query(role_id);
        let (sql_del, _) = compile_delete(&stmt_del);
        assert!(sql_del.contains("DELETE FROM \"role_permissions\" WHERE \"role_id\" = $1"));

        assert!(insert_permissions_batch_query(role_id, &[]).is_none());

        let perms = [
            ("ManageSchema", None),
            ("CreateDocument", Some("article")),
        ];
        let stmt_batch = insert_permissions_batch_query(role_id, &perms).unwrap();
        let unparam_batch = stmt_batch.to_string(PostgresQueryBuilder);
        assert!(unparam_batch.contains("INSERT INTO \"role_permissions\""));
        assert!(unparam_batch.contains("'ManageSchema'"));
        assert!(unparam_batch.contains("'article'"));
    }

    #[test]
    fn test_user_role_assignment_queries() {
        let user_id = UserId::try_new("alice").unwrap();
        let admin_role_id = Uuid::now_v7();

        let stmt_sel = select_assignments_by_user_query(&user_id);
        let (sql_sel, _) = compile_select(&stmt_sel);
        assert!(sql_sel.contains("FROM \"user_role_assignments\" WHERE \"user_id\" = $1"));
        assert!(sql_sel.contains("ORDER BY \"granted_at\" ASC, \"id\" ASC"));

        let stmt_exists = exists_admin_assignment_query(admin_role_id);
        let unparam_exists = stmt_exists.to_string(PostgresQueryBuilder);
        assert!(unparam_exists.contains("SELECT 1 FROM \"user_role_assignments\""));
        assert!(unparam_exists.contains("LIMIT 1"));

        let assignment = UserRoleAssignment {
            id: UserRoleAssignmentId::new(Uuid::now_v7()),
            user_id: user_id.clone(),
            role_id: RoleId::new(admin_role_id),
            granted_at: Utc::now(),
            granted_by: Some(ActorId::System),
        };
        let stmt_upsert = upsert_assignment_query(&assignment);
        let unparam_upsert = stmt_upsert.to_string(PostgresQueryBuilder);
        assert!(unparam_upsert.contains("INSERT INTO \"user_role_assignments\""));
        assert!(unparam_upsert.contains("ON CONFLICT (\"user_id\", \"role_id\") DO UPDATE SET"));

        let stmt_del = delete_assignment_query(*assignment.id);
        let (sql_del, _) = compile_delete(&stmt_del);
        assert!(sql_del.contains("DELETE FROM \"user_role_assignments\" WHERE \"id\" = $1"));
    }

    #[test]
    fn test_access_request_queries() {
        let req_id = Uuid::now_v7();
        let user_id = UserId::try_new("bob").unwrap();

        let stmt_by_id = select_access_request_by_id_query(req_id);
        let (sql_by_id, _) = compile_select(&stmt_by_id);
        assert!(sql_by_id.contains("FROM \"access_requests\" WHERE \"id\" = $1"));

        let stmt_latest = select_latest_access_request_by_user_query(&user_id);
        let unparam_latest = stmt_latest.to_string(PostgresQueryBuilder);
        assert!(unparam_latest.contains("ORDER BY \"requested_at\" DESC LIMIT 1"));

        let stmt_active = select_active_access_request_by_user_query(&user_id);
        let unparam_active = stmt_active.to_string(PostgresQueryBuilder);
        assert!(unparam_active.contains("\"status\" IN ('pending', 'approved')"));
        assert!(unparam_active.contains("ORDER BY \"requested_at\" DESC LIMIT 1"));

        let stmt_pending = select_pending_access_requests_query();
        let unparam_pending = stmt_pending.to_string(PostgresQueryBuilder);
        assert!(unparam_pending.contains("\"status\" = 'pending'"));
        assert!(unparam_pending.contains("ORDER BY \"requested_at\" ASC"));

        let request = AccessRequest {
            id: AccessRequestId::new(req_id),
            user_id: user_id.clone(),
            email: Some(Email::try_new("bob@example.com").unwrap()),
            name: Some(DisplayName::try_new("Bob").unwrap()),
            requested_at: Utc::now(),
            status: AccessRequestStatus::Pending,
            reviewed_by: None,
            reviewed_at: None,
            assigned_roles: vec![],
        };
        let stmt_upsert = upsert_access_request_query(&request, "pending", None, json!([]));
        let unparam_upsert = stmt_upsert.to_string(PostgresQueryBuilder);
        assert!(unparam_upsert.contains("INSERT INTO \"access_requests\""));
        assert!(unparam_upsert.contains("ON CONFLICT (\"id\") DO UPDATE SET"));
        assert!(unparam_upsert.contains("\"status\" = \"excluded\".\"status\""));
    }

    #[test]
    fn test_user_profile_queries() {
        let user_id = UserId::try_new("alice_sub").unwrap();
        let profile = UserProfile::new(
            user_id.clone(),
            Email::try_new("alice@example.com").ok(),
            DisplayName::try_new("Alice").ok(),
            Utc::now(),
        );

        // 1. Select by ID
        let stmt_by_id = select_user_profile_by_id_query(&user_id);
        let (sql_by_id, _) = crate::persistence::query::common::compile_select(&stmt_by_id);
        assert!(sql_by_id.contains("FROM \"user_profiles\" WHERE \"user_id\" = $1"));

        // 2. Select by IDs
        let stmt_by_ids = select_user_profiles_by_ids_query(std::slice::from_ref(&user_id));
        let (sql_by_ids, _) = crate::persistence::query::common::compile_select(&stmt_by_ids);
        assert!(sql_by_ids.contains("FROM \"user_profiles\" WHERE \"user_id\" IN ($1)"));

        // 3. Paginated list with search
        let stmt_list = select_user_profiles_paginated_query(
            Some("alice"),
            Pagination::new(1, 10),
        );
        let unparam_list = stmt_list.to_string(PostgresQueryBuilder);
        assert!(unparam_list.contains("FROM \"user_profiles\""));
        assert!(unparam_list.contains("ILIKE '%alice%'"));
        assert!(unparam_list.contains("ORDER BY \"user_id\" ASC LIMIT 10"));

        // 4. Count query
        let stmt_count = count_user_profiles_query(Some("alice"));
        let unparam_count = stmt_count.to_string(PostgresQueryBuilder);
        assert!(unparam_count.contains("COUNT(\"user_id\")"));
        assert!(unparam_count.contains("ILIKE '%alice%'"));

        // 5. Upsert query
        let stmt_upsert = upsert_user_profile_query(&profile);
        let unparam_upsert = stmt_upsert.to_string(PostgresQueryBuilder);
        assert!(unparam_upsert.contains("INSERT INTO \"user_profiles\""));
        assert!(unparam_upsert.contains("ON CONFLICT (\"user_id\") DO UPDATE SET"));
        assert!(unparam_upsert.contains("\"email\" = \"excluded\".\"email\""));
        assert!(unparam_upsert.contains("\"name\" = \"excluded\".\"name\""));
        assert!(unparam_upsert.contains("\"last_seen_at\" = \"excluded\".\"last_seen_at\""));
    }
}
