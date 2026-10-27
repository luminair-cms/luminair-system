//! Local provisioning and tracking of verified OIDC identities.

use std::future::Future;

use chrono::Utc;
use sqlx::PgPool;

use domain::errors::DomainError;

use crate::auth::claims::AuthenticatedUser;

// ---------------------------------------------------------------------------
// Traits & Types
// ---------------------------------------------------------------------------

/// Port trait for recording observed user logins into the local identity cache.
pub trait ShadowUserRecorder: Send + Sync + 'static {
    fn record_user_seen(
        &self,
        user: &AuthenticatedUser,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;
}

/// No-op recorder that discards user-seen events without database operations.
///
/// Useful for unit tests, offline CLI commands, and ephemeral execution modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoopShadowUserRecorder;

/// Database-backed recorder that upserts seen users into the `shadow_users` table.
#[derive(Debug, Clone)]
pub struct SqlxShadowUserRecorder {
    pool: PgPool,
}

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

impl SqlxShadowUserRecorder {
    /// Creates a new recorder connected to the given database pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ShadowUserRecorder for NoopShadowUserRecorder {
    async fn record_user_seen(&self, _user: &AuthenticatedUser) -> Result<(), DomainError> {
        Ok(())
    }
}

impl ShadowUserRecorder for SqlxShadowUserRecorder {
    async fn record_user_seen(&self, user: &AuthenticatedUser) -> Result<(), DomainError> {
        let user_id: &str = user.user_id.as_ref();
        let email: Option<&str> = user.email.as_ref().map(AsRef::as_ref);
        let name: Option<&str> = user.name.as_ref().map(AsRef::as_ref);
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO shadow_users (user_id, email, name, auth_type, first_seen, last_seen)
            VALUES ($1, $2, $3, 'oidc', $4, $4)
            ON CONFLICT (user_id) DO UPDATE SET
                email = COALESCE(EXCLUDED.email, shadow_users.email),
                name = COALESCE(EXCLUDED.name, shadow_users.name),
                last_seen = EXCLUDED.last_seen
            "#,
        )
        .bind(user_id)
        .bind(email)
        .bind(name)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|err| DomainError::Storage(err.to_string()))?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::auth::user::UserId;
    use domain::common::{DisplayName, Email};

    #[tokio::test]
    async fn test_noop_recorder_returns_ok() {
        let recorder = NoopShadowUserRecorder;
        let user_id = UserId::try_new("usr_test").expect("valid user");
        let email = Email::try_new("user@example.com").ok();
        let name = DisplayName::try_new("Test User").ok();
        let user = AuthenticatedUser::new(user_id, email, name);

        let res = recorder.record_user_seen(&user).await;
        assert!(res.is_ok());
    }
}
