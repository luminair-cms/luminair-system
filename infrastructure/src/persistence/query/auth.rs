//! Sea-Query dynamic query builders for RBAC (`roles`, `role_permissions`, `user_role_assignments`) and `access_requests`.

use sea_query::{
    DeleteStatement, Expr, ExprTrait, InsertStatement, OnConflict, Order, Query, SelectStatement,
};
use uuid::Uuid;

use domain::auth::access_request::AccessRequest;
use domain::auth::role::{Role, UserRoleAssignment};
use domain::auth::user::UserId;

use crate::persistence::naming::StaticIden;
use super::common::{
    IDEN_ACTION, IDEN_ASSIGNED_ROLES, IDEN_CREATED_AT, IDEN_DESCRIPTION, IDEN_DOCUMENT_TYPE_ID,
    IDEN_EMAIL, IDEN_GRANTED_AT, IDEN_GRANTED_BY, IDEN_ID, IDEN_NAME, IDEN_REJECTION_REASON,
    IDEN_REQUESTED_AT, IDEN_REVIEWED_AT, IDEN_REVIEWED_BY, IDEN_ROLE_ID, IDEN_STATUS,
    IDEN_UPDATED_AT, IDEN_USER_ID, TABLE_ACCESS_REQUESTS_IDEN, TABLE_ROLE_PERMISSIONS_IDEN,
    TABLE_ROLES_IDEN, TABLE_USER_ROLE_ASSIGNMENTS_IDEN,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const ROLE_COLUMNS: [StaticIden; 3] = [IDEN_ID, IDEN_NAME, IDEN_DESCRIPTION];

const PERMISSION_COLUMNS: [StaticIden; 3] = [
    IDEN_ROLE_ID,
    IDEN_ACTION,
    IDEN_DOCUMENT_TYPE_ID,
];

const PERMISSION_INSERT_COLUMNS: [StaticIden; 4] = [
    IDEN_ID,
    IDEN_ROLE_ID,
    IDEN_ACTION,
    IDEN_DOCUMENT_TYPE_ID,
];

const USER_ROLE_ASSIGNMENT_COLUMNS: [StaticIden; 5] = [
    IDEN_ID,
    IDEN_USER_ID,
    IDEN_ROLE_ID,
    IDEN_GRANTED_AT,
    IDEN_GRANTED_BY,
];

const ACCESS_REQUEST_COLUMNS: [StaticIden; 10] = [
    IDEN_ID,
    IDEN_USER_ID,
    IDEN_EMAIL,
    IDEN_NAME,
    IDEN_REQUESTED_AT,
    IDEN_STATUS,
    IDEN_REJECTION_REASON,
    IDEN_REVIEWED_BY,
    IDEN_REVIEWED_AT,
    IDEN_ASSIGNED_ROLES,
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
        .and_where(Expr::col(IDEN_ID).eq(id));
    stmt
}

/// Builds a `SELECT id, name, description FROM roles WHERE id IN (...) ORDER BY name ASC` query.
#[must_use]
pub fn select_roles_by_ids_query(ids: &[Uuid]) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLES_IDEN)
        .columns(ROLE_COLUMNS)
        .and_where(Expr::col(IDEN_ID).is_in(ids.to_vec()))
        .order_by(IDEN_NAME, Order::Asc);
    stmt
}

/// Builds a `SELECT id, name, description FROM roles WHERE name = $1` query.
#[must_use]
pub fn select_role_by_name_query(name: &str) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLES_IDEN)
        .columns(ROLE_COLUMNS)
        .and_where(Expr::col(IDEN_NAME).eq(name));
    stmt
}

/// Builds a `SELECT id, name, description FROM roles ORDER BY name ASC` query.
#[must_use]
pub fn select_all_roles_query() -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLES_IDEN)
        .columns(ROLE_COLUMNS)
        .order_by(IDEN_NAME, Order::Asc);
    stmt
}

/// Builds a query to fetch all permissions for a set of role IDs ordered by creation time.
#[must_use]
pub fn select_permissions_by_role_ids_query(role_ids: &[Uuid]) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ROLE_PERMISSIONS_IDEN)
        .columns(PERMISSION_COLUMNS)
        .and_where(Expr::col(IDEN_ROLE_ID).is_in(role_ids.to_vec()))
        .order_by(IDEN_CREATED_AT, Order::Asc)
        .order_by(IDEN_ID, Order::Asc);
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
            OnConflict::column(IDEN_ID)
                .update_columns([IDEN_NAME, IDEN_DESCRIPTION])
                .value(IDEN_UPDATED_AT, Expr::current_timestamp())
                .to_owned(),
        );
    stmt
}

/// Builds a `DELETE FROM role_permissions WHERE role_id = $1` statement.
#[must_use]
pub fn delete_permissions_by_role_query(role_id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(TABLE_ROLE_PERMISSIONS_IDEN)
        .and_where(Expr::col(IDEN_ROLE_ID).eq(role_id));
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
        .and_where(Expr::col(IDEN_USER_ID).eq(user_id.as_ref()))
        .order_by(IDEN_GRANTED_AT, Order::Asc)
        .order_by(IDEN_ID, Order::Asc);
    stmt
}

/// Builds an existence check for any user assignment with the given admin role ID.
#[must_use]
pub fn exists_admin_assignment_query(admin_role_id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_USER_ROLE_ASSIGNMENTS_IDEN)
        .expr(Expr::val(1))
        .and_where(Expr::col(IDEN_ROLE_ID).eq(admin_role_id))
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
            OnConflict::columns([IDEN_USER_ID, IDEN_ROLE_ID])
                .update_columns([IDEN_GRANTED_AT, IDEN_GRANTED_BY])
                .to_owned(),
        );
    stmt
}

/// Builds a `DELETE FROM user_role_assignments WHERE id = $1` statement.
#[must_use]
pub fn delete_assignment_query(id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(TABLE_USER_ROLE_ASSIGNMENTS_IDEN)
        .and_where(Expr::col(IDEN_ID).eq(id));
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
        .and_where(Expr::col(IDEN_ID).eq(id));
    stmt
}

/// Builds a query to fetch the latest access request submitted by a user.
#[must_use]
pub fn select_latest_access_request_by_user_query(user_id: &UserId) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ACCESS_REQUESTS_IDEN)
        .columns(ACCESS_REQUEST_COLUMNS)
        .and_where(Expr::col(IDEN_USER_ID).eq(user_id.as_ref()))
        .order_by(IDEN_REQUESTED_AT, Order::Desc)
        .limit(1);
    stmt
}

/// Builds a query to fetch an active (`pending` or `approved`) access request for a user.
#[must_use]
pub fn select_active_access_request_by_user_query(user_id: &UserId) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ACCESS_REQUESTS_IDEN)
        .columns(ACCESS_REQUEST_COLUMNS)
        .and_where(Expr::col(IDEN_USER_ID).eq(user_id.as_ref()))
        .and_where(Expr::col(IDEN_STATUS).is_in(["pending", "approved"]))
        .order_by(IDEN_REQUESTED_AT, Order::Desc)
        .limit(1);
    stmt
}

/// Builds a query to fetch all pending access requests ordered by request time ascending.
#[must_use]
pub fn select_pending_access_requests_query() -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_ACCESS_REQUESTS_IDEN)
        .columns(ACCESS_REQUEST_COLUMNS)
        .and_where(Expr::col(IDEN_STATUS).eq("pending"))
        .order_by(IDEN_REQUESTED_AT, Order::Asc);
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
            OnConflict::column(IDEN_ID)
                .update_columns([
                    IDEN_STATUS,
                    IDEN_REJECTION_REASON,
                    IDEN_REVIEWED_BY,
                    IDEN_REVIEWED_AT,
                    IDEN_ASSIGNED_ROLES,
                ])
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
}
