//! SQLx implementation of `UserProfileRepository`.

use sqlx::{AssertSqlSafe, PgPool, Row};

use domain::auth::ports::UserProfileRepository;
use domain::auth::profile::UserProfile;
use domain::auth::user::UserId;
use domain::common::{Page, Pagination};
use domain::errors::DomainError;

use super::mappers::{map_user_profile_row, map_user_profile_rows};
use crate::persistence::error::PersistenceError;
use crate::persistence::query::auth::{
    count_user_profiles_query, select_user_profile_by_id_query,
    select_user_profiles_by_ids_query, select_user_profiles_paginated_query,
    upsert_user_profile_query,
};
use crate::persistence::query::common::{compile_insert, compile_select};
use crate::persistence::query::occ::with_occ_retry;

// ---------------------------------------------------------------------------
// Public Repository Struct & Constructors
// ---------------------------------------------------------------------------

/// PostgreSQL / Aurora DSQL adapter for `UserProfileRepository`.
#[derive(Debug, Clone)]
pub struct SqlxUserProfileRepository {
    pool: PgPool,
}

impl SqlxUserProfileRepository {
    /// Creates a new `SqlxUserProfileRepository` using the given connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementation: UserProfileRepository
// ---------------------------------------------------------------------------

impl UserProfileRepository for SqlxUserProfileRepository {
    async fn find_by_id(&self, user_id: &UserId) -> Result<Option<UserProfile>, DomainError> {
        let (sql, values) = compile_select(&select_user_profile_by_id_query(user_id));
        let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let Some(row) = maybe_row else {
            return Ok(None);
        };

        let profile = map_user_profile_row(&row)?;
        Ok(Some(profile))
    }

    async fn find_by_ids(&self, user_ids: &[UserId]) -> Result<Vec<UserProfile>, DomainError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let (sql, values) = compile_select(&select_user_profiles_by_ids_query(user_ids));
        let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_all(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let profiles = map_user_profile_rows(&rows)?;
        Ok(profiles)
    }

    async fn list_users(
        &self,
        query: Option<&str>,
        pagination: Pagination,
    ) -> Result<Page<UserProfile>, DomainError> {
        let (count_sql, count_values) = compile_select(&count_user_profiles_query(query));
        let count_row = sqlx::query_with(AssertSqlSafe(count_sql.as_str()), count_values)
            .fetch_one(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let total: i64 = count_row.try_get(0).map_err(PersistenceError::from)?;

        let (list_sql, list_values) =
            compile_select(&select_user_profiles_paginated_query(query, pagination));
        let rows = sqlx::query_with(AssertSqlSafe(list_sql.as_str()), list_values)
            .fetch_all(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        let items = map_user_profile_rows(&rows)?;
        Ok(Page::new(items, total as u64, pagination.page, pagination.page_size))
    }

    async fn save(&self, profile: &UserProfile) -> Result<(), DomainError> {
        with_occ_retry(|| async { execute_upsert_user_profile(&self.pool, profile).await })
            .await
            .map_err(DomainError::from)
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

async fn execute_upsert_user_profile(
    pool: &PgPool,
    profile: &UserProfile,
) -> Result<(), PersistenceError> {
    let (sql, values) = compile_insert(&upsert_user_profile_query(profile));
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(pool)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlx_user_profile_repository_new() {
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test")
            .expect("lazy pool creation must succeed");
        let repo = SqlxUserProfileRepository::new(pool);
        assert!(format!("{repo:?}").contains("SqlxUserProfileRepository"));
    }
}
