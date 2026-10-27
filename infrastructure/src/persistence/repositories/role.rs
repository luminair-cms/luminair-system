//! SQLx implementation of `RoleRepository`.

use sqlx::postgres::PgRow;
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use uuid::Uuid;

use domain::auth::ports::RoleRepository;
use domain::auth::role::{Permission, Role, RoleId};
use domain::errors::DomainError;

use crate::persistence::error::PersistenceError;
use crate::persistence::query::auth::{
    delete_permissions_by_role_query, insert_permissions_batch_query, select_all_roles_query,
    select_permissions_by_role_ids_query, select_role_by_id_query, select_role_by_name_query,
    select_roles_by_ids_query, upsert_role_query,
};
use crate::persistence::query::common::{compile_delete, compile_insert, compile_select};
use crate::persistence::query::occ::with_occ_retry;
use super::codec::permission_to_db_tuple;
use super::mappers::{assemble_roles, map_permission_row, map_role_base_row};

// ---------------------------------------------------------------------------
// Public Repository Struct & Constructors
// ---------------------------------------------------------------------------

/// PostgreSQL / Aurora DSQL adapter for `RoleRepository`.
#[derive(Debug, Clone)]
pub struct SqlxRoleRepository {
    pool: PgPool,
}

impl SqlxRoleRepository {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementation: RoleRepository
// ---------------------------------------------------------------------------

impl RoleRepository for SqlxRoleRepository {
    async fn find_by_id(&self, id: RoleId) -> Result<Option<Role>, DomainError> {
        let (sql, values) = compile_select(&select_role_by_id_query(*id));
        let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let Some(row) = maybe_row else {
            return Ok(None);
        };

        let base = map_role_base_row(&row)?;
        let perms = fetch_permissions_for_roles(&self.pool, &[*id]).await?;
        let role = assemble_roles(vec![base], perms).into_iter().next();
        Ok(role)
    }

    async fn find_by_ids(&self, ids: &[RoleId]) -> Result<Vec<Role>, DomainError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let uuids: Vec<Uuid> = ids.iter().map(|id| **id).collect();
        let (sql, values) = compile_select(&select_roles_by_ids_query(&uuids));
        let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_all(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let base_rows = map_role_base_rows(&rows)?;
        let perms = fetch_permissions_for_roles(&self.pool, &uuids).await?;
        Ok(assemble_roles(base_rows, perms))
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>, DomainError> {
        let (sql, values) = compile_select(&select_role_by_name_query(name));
        let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let Some(row) = maybe_row else {
            return Ok(None);
        };

        let base = map_role_base_row(&row)?;
        let perms = fetch_permissions_for_roles(&self.pool, &[*base.0]).await?;
        let role = assemble_roles(vec![base], perms).into_iter().next();
        Ok(role)
    }

    async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
        let (sql, values) = compile_select(&select_all_roles_query());
        let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_all(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let base_rows = map_role_base_rows(&rows)?;
        if base_rows.is_empty() {
            return Ok(Vec::new());
        }

        let uuids = extract_role_uuids(&base_rows);
        let perms = fetch_permissions_for_roles(&self.pool, &uuids).await?;
        Ok(assemble_roles(base_rows, perms))
    }

    async fn save(&self, role: &Role) -> Result<(), DomainError> {
        with_occ_retry(|| async { save_role_in_tx(&self.pool, role).await })
            .await
            .map_err(DomainError::from)
    }
}

// ---------------------------------------------------------------------------
// Private Helpers: Role Queries & Transaction Operations
// ---------------------------------------------------------------------------

async fn fetch_permissions_for_roles(
    pool: &PgPool,
    role_ids: &[Uuid],
) -> Result<Vec<(RoleId, Permission)>, PersistenceError> {
    if role_ids.is_empty() {
        return Ok(Vec::new());
    }
    let (sql, values) = compile_select(&select_permissions_by_role_ids_query(role_ids));
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;
    rows.iter().map(map_permission_row).collect()
}

fn map_role_base_rows(
    rows: &[PgRow],
) -> Result<Vec<(RoleId, String, Option<String>)>, PersistenceError> {
    rows.iter().map(map_role_base_row).collect()
}

fn extract_role_uuids(base_rows: &[(RoleId, String, Option<String>)]) -> Vec<Uuid> {
    base_rows.iter().map(|(id, _, _)| **id).collect()
}

async fn save_role_in_tx(pool: &PgPool, role: &Role) -> Result<(), PersistenceError> {
    let mut tx = pool.begin().await?;
    execute_upsert_role(&mut tx, role).await?;
    execute_delete_role_permissions(&mut tx, *role.id).await?;
    execute_insert_role_permissions(&mut tx, *role.id, &role.permissions).await?;
    tx.commit().await?;
    Ok(())
}

async fn execute_upsert_role(
    conn: &mut PgConnection,
    role: &Role,
) -> Result<(), PersistenceError> {
    let (sql, values) = compile_insert(&upsert_role_query(role));
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(conn)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

async fn execute_delete_role_permissions(
    conn: &mut PgConnection,
    role_id: Uuid,
) -> Result<(), PersistenceError> {
    let (sql, values) = compile_delete(&delete_permissions_by_role_query(role_id));
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(conn)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

async fn execute_insert_role_permissions(
    conn: &mut PgConnection,
    role_id: Uuid,
    permissions: &[Permission],
) -> Result<(), PersistenceError> {
    if permissions.is_empty() {
        return Ok(());
    }
    let tuples: Vec<(&str, Option<&str>)> = build_permission_tuples(permissions);
    if let Some(stmt) = insert_permissions_batch_query(role_id, &tuples) {
        let (sql, values) = compile_insert(&stmt);
        sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .execute(conn)
            .await
            .map_err(PersistenceError::from)?;
    }
    Ok(())
}

fn build_permission_tuples(
    permissions: &[Permission],
) -> Vec<(&'static str, Option<&str>)> {
    permissions.iter().map(permission_to_db_tuple).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlx_role_repository_new() {
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test")
            .expect("lazy pool creation must succeed");
        let repo = SqlxRoleRepository::new(pool);
        assert!(format!("{repo:?}").contains("SqlxRoleRepository"));
    }

    #[test]
    fn test_extract_role_uuids() {
        let u1 = Uuid::now_v7();
        let u2 = Uuid::now_v7();
        let base_rows = vec![
            (RoleId::new(u1), "admin".to_string(), None),
            (RoleId::new(u2), "editor".to_string(), Some("desc".to_string())),
        ];
        let uuids = extract_role_uuids(&base_rows);
        assert_eq!(uuids, vec![u1, u2]);
    }

    #[test]
    fn test_build_permission_tuples() {
        let permissions = vec![
            Permission::ManageSchema,
            Permission::CreateDocument(None),
        ];
        let tuples = build_permission_tuples(&permissions);
        assert_eq!(tuples.len(), 2);
        assert_eq!(tuples[0].0, "ManageSchema");
        assert_eq!(tuples[0].1, None);
        assert_eq!(tuples[1].0, "CreateDocument");
        assert_eq!(tuples[1].1, None);
    }
}
