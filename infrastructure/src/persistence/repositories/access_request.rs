//! SQLx implementation of `AccessRequestRepository`.

use sqlx::postgres::PgRow;
use sqlx::{AssertSqlSafe, PgPool};

use domain::auth::access_request::{AccessRequest, AccessRequestId};
use domain::auth::ports::AccessRequestRepository;
use domain::auth::user::UserId;
use domain::errors::DomainError;

use crate::persistence::error::PersistenceError;
use crate::persistence::query::auth::{
    select_access_request_by_id_query, select_active_access_request_by_user_query,
    select_latest_access_request_by_user_query, select_pending_access_requests_query,
    upsert_access_request_query,
};
use crate::persistence::query::common::{compile_insert, compile_select};
use crate::persistence::query::occ::with_occ_retry;
use super::codec::{access_request_status_to_db, serialize_assigned_roles_json};
use super::mappers::map_access_request_row;

// ---------------------------------------------------------------------------
// Public Repository Struct & Constructors
// ---------------------------------------------------------------------------

/// PostgreSQL / Aurora DSQL adapter for `AccessRequestRepository`.
#[derive(Debug, Clone)]
pub struct SqlxAccessRequestRepository {
    pool: PgPool,
}

impl SqlxAccessRequestRepository {
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementation: AccessRequestRepository
// ---------------------------------------------------------------------------

impl AccessRequestRepository for SqlxAccessRequestRepository {
    async fn find_by_id(
        &self,
        id: AccessRequestId,
    ) -> Result<Option<AccessRequest>, DomainError> {
        let (sql, values) = compile_select(&select_access_request_by_id_query(*id));
        let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let Some(row) = maybe_row else {
            return Ok(None);
        };

        let request = map_access_request_row(&row)?;
        Ok(Some(request))
    }

    async fn find_by_user(
        &self,
        user_id: &UserId,
    ) -> Result<Option<AccessRequest>, DomainError> {
        let (sql, values) = compile_select(&select_latest_access_request_by_user_query(user_id));
        let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let Some(row) = maybe_row else {
            return Ok(None);
        };

        let request = map_access_request_row(&row)?;
        Ok(Some(request))
    }

    async fn find_active_by_user(
        &self,
        user_id: &UserId,
    ) -> Result<Option<AccessRequest>, DomainError> {
        let (sql, values) = compile_select(&select_active_access_request_by_user_query(user_id));
        let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let Some(row) = maybe_row else {
            return Ok(None);
        };

        let request = map_access_request_row(&row)?;
        Ok(Some(request))
    }

    async fn find_pending(&self) -> Result<Vec<AccessRequest>, DomainError> {
        let (sql, values) = compile_select(&select_pending_access_requests_query());
        let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_all(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let requests = map_access_request_rows(&rows)?;
        Ok(requests)
    }

    async fn save(&self, request: &AccessRequest) -> Result<(), DomainError> {
        with_occ_retry(|| async { execute_upsert_access_request(&self.pool, request).await })
            .await
            .map_err(DomainError::from)
    }
}

// ---------------------------------------------------------------------------
// Private Helpers: Access Request Queries & Mappings
// ---------------------------------------------------------------------------

fn map_access_request_rows(rows: &[PgRow]) -> Result<Vec<AccessRequest>, PersistenceError> {
    rows.iter().map(map_access_request_row).collect()
}

async fn execute_upsert_access_request(
    pool: &PgPool,
    request: &AccessRequest,
) -> Result<(), PersistenceError> {
    let (status_str, rejection_reason) = access_request_status_to_db(&request.status);
    let assigned_roles_json = serialize_assigned_roles_json(&request.assigned_roles)?;
    let stmt = upsert_access_request_query(
        request,
        status_str,
        rejection_reason,
        assigned_roles_json,
    );
    let (sql, values) = compile_insert(&stmt);
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
    async fn test_sqlx_access_request_repository_new() {
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test")
            .expect("lazy pool creation must succeed");
        let repo = SqlxAccessRequestRepository::new(pool);
        assert!(format!("{repo:?}").contains("SqlxAccessRequestRepository"));
    }
}
