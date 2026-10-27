//! SQLx implementation of `UserRoleAssignmentRepository`.

use sqlx::postgres::PgRow;
use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use uuid::Uuid;

use domain::auth::ports::UserRoleAssignmentRepository;
use domain::auth::role::{RoleId, UserRoleAssignment, UserRoleAssignmentId};
use domain::auth::user::UserId;
use domain::errors::DomainError;

use crate::persistence::error::PersistenceError;
use crate::persistence::query::auth::{
    delete_assignment_query, exists_admin_assignment_query, select_assignments_by_user_query,
    upsert_assignment_query,
};
use crate::persistence::query::common::{compile_delete, compile_insert, compile_select};
use crate::persistence::query::occ::with_occ_retry;
use super::mappers::map_user_role_assignment_row;

// ---------------------------------------------------------------------------
// Public Repository Struct & Constructors
// ---------------------------------------------------------------------------

/// PostgreSQL / Aurora DSQL adapter for `UserRoleAssignmentRepository`.
#[derive(Debug, Clone)]
pub struct SqlxUserRoleAssignmentRepository {
    pool: PgPool,
}

impl SqlxUserRoleAssignmentRepository {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementation: UserRoleAssignmentRepository
// ---------------------------------------------------------------------------

impl UserRoleAssignmentRepository for SqlxUserRoleAssignmentRepository {
    async fn find_by_user(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<UserRoleAssignment>, DomainError> {
        let (sql, values) = compile_select(&select_assignments_by_user_query(user_id));
        let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_all(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let assignments = map_user_role_assignment_rows(&rows)?;
        Ok(assignments)
    }

    async fn exists_admin(&self, admin_role_id: RoleId) -> Result<bool, DomainError> {
        let (sql, values) = compile_select(&exists_admin_assignment_query(*admin_role_id));
        let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        Ok(maybe_row.is_some())
    }

    async fn save(&self, assignment: &UserRoleAssignment) -> Result<(), DomainError> {
        with_occ_retry(|| async { execute_upsert_assignment(&self.pool, assignment).await })
            .await
            .map_err(DomainError::from)
    }

    async fn save_all(&self, assignments: &[UserRoleAssignment]) -> Result<(), DomainError> {
        if assignments.is_empty() {
            return Ok(());
        }

        with_occ_retry(|| async { save_assignments_in_tx(&self.pool, assignments).await })
            .await
            .map_err(DomainError::from)
    }

    async fn delete(&self, id: UserRoleAssignmentId) -> Result<(), DomainError> {
        with_occ_retry(|| async { execute_delete_assignment(&self.pool, *id).await })
            .await
            .map_err(DomainError::from)
    }
}

// ---------------------------------------------------------------------------
// Private Helpers: User Role Assignment Queries & Transactions
// ---------------------------------------------------------------------------

fn map_user_role_assignment_rows(
    rows: &[PgRow],
) -> Result<Vec<UserRoleAssignment>, PersistenceError> {
    rows.iter().map(map_user_role_assignment_row).collect()
}

async fn execute_upsert_assignment(
    pool: &PgPool,
    assignment: &UserRoleAssignment,
) -> Result<(), PersistenceError> {
    let (sql, values) = compile_insert(&upsert_assignment_query(assignment));
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(pool)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

async fn save_assignments_in_tx(
    pool: &PgPool,
    assignments: &[UserRoleAssignment],
) -> Result<(), PersistenceError> {
    let mut tx = pool.begin().await?;
    for assignment in assignments {
        execute_upsert_assignment_in_tx(&mut tx, assignment).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn execute_upsert_assignment_in_tx(
    conn: &mut PgConnection,
    assignment: &UserRoleAssignment,
) -> Result<(), PersistenceError> {
    let (sql, values) = compile_insert(&upsert_assignment_query(assignment));
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(conn)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

async fn execute_delete_assignment(
    pool: &PgPool,
    id: Uuid,
) -> Result<(), PersistenceError> {
    let (sql, values) = compile_delete(&delete_assignment_query(id));
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(pool)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlx_user_role_assignment_repository_new() {
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test")
            .expect("lazy pool creation must succeed");
        let repo = SqlxUserRoleAssignmentRepository::new(pool);
        assert!(format!("{repo:?}").contains("SqlxUserRoleAssignmentRepository"));
    }
}
