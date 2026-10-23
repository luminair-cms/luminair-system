//! Optimistic Concurrency Control (OCC) retry subsystem for AWS Aurora DSQL.
//!
//! Catches transaction conflicts emitting SQLSTATE `40001` (`serialization_failure`)
//! and retries execution with randomized exponential backoff and jitter.

use std::future::Future;
use std::time::Duration;

use crate::persistence::error::PersistenceError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const DEFAULT_MAX_RETRIES: usize = 3;
pub const DEFAULT_BASE_DELAY_MS: u64 = 25;
pub const DEFAULT_MAX_DELAY_MS: u64 = 500;

// ---------------------------------------------------------------------------
// Public Retry Functions
// ---------------------------------------------------------------------------

/// Wraps an asynchronous operation in an OCC retry loop using default retry settings.
///
/// # Errors
/// Returns [`PersistenceError::OccConflictExhausted`] if retry budget is depleted,
/// or bubbles up non-retryable errors immediately.
pub async fn with_occ_retry<F, Fut, T>(op: F) -> Result<T, PersistenceError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, PersistenceError>>,
{
    with_occ_retry_opts(
        DEFAULT_MAX_RETRIES,
        DEFAULT_BASE_DELAY_MS,
        DEFAULT_MAX_DELAY_MS,
        op,
    )
    .await
}

/// Wraps an asynchronous operation in an OCC retry loop with customizable backoff bounds.
///
/// # Errors
/// Returns [`PersistenceError::OccConflictExhausted`] if retry budget is depleted,
/// or bubbles up non-retryable errors immediately.
pub async fn with_occ_retry_opts<F, Fut, T>(
    max_retries: usize,
    base_delay_ms: u64,
    max_delay_ms: u64,
    mut op: F,
) -> Result<T, PersistenceError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, PersistenceError>>,
{
    for attempt in 0..=max_retries {
        match op().await {
            Ok(val) => return Ok(val),
            Err(err) if is_serialization_failure(&err) => {
                if attempt < max_retries {
                    let sleep_duration = compute_backoff(attempt, base_delay_ms, max_delay_ms);
                    tokio::time::sleep(sleep_duration).await;
                }
            }
            Err(err) => return Err(err),
        }
    }

    Err(PersistenceError::OccConflictExhausted {
        attempts: max_retries + 1,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Computes the exponential backoff duration with randomized entropy jitter.
fn compute_backoff(attempt: usize, base_delay_ms: u64, max_delay_ms: u64) -> Duration {
    let exp = u32::try_from(attempt).unwrap_or(u32::MAX);
    let factor = 2_u64.saturating_pow(exp);
    let delay = std::cmp::min(max_delay_ms, base_delay_ms.saturating_mul(factor));
    let jitter = if delay > 1 {
        let span = u128::from(delay / 2 + 1);
        u64::try_from(uuid::Uuid::new_v4().as_u128() % span).unwrap_or(0)
    } else {
        0
    };
    Duration::from_millis(delay.saturating_add(jitter))
}

/// Checks whether the given error represents a PostgreSQL/DSQL serialization failure (SQLSTATE 40001).
fn is_serialization_failure(err: &PersistenceError) -> bool {
    match err {
        PersistenceError::Sqlx(sqlx::Error::Database(db_err)) => {
            db_err.code().as_deref() == Some("40001")
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct MockDbError {
        code: &'static str,
    }

    impl std::fmt::Display for MockDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "mock database error: code {}", self.code)
        }
    }

    impl std::error::Error for MockDbError {}

    impl sqlx::error::DatabaseError for MockDbError {
        fn message(&self) -> &'static str {
            "mock db error"
        }
        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            Some(std::borrow::Cow::Borrowed(self.code))
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
    }

    fn make_occ_error() -> PersistenceError {
        PersistenceError::Sqlx(sqlx::Error::Database(Box::new(MockDbError {
            code: "40001",
        })))
    }

    fn make_other_error() -> PersistenceError {
        PersistenceError::Sqlx(sqlx::Error::Database(Box::new(MockDbError {
            code: "23505",
        })))
    }

    #[tokio::test]
    async fn test_with_occ_retry_success_first_attempt() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = with_occ_retry(|| {
            let a = attempts_clone.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Ok::<_, PersistenceError>(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_occ_retry_retries_on_40001_and_succeeds() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result = with_occ_retry_opts(3, 1, 5, || {
            let a = attempts_clone.clone();
            async move {
                let current = a.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    Err(make_occ_error())
                } else {
                    Ok(100)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 100);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_occ_retry_exhausted_returns_conflict_error() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<(), PersistenceError> = with_occ_retry_opts(2, 1, 5, || {
            let a = attempts_clone.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Err(make_occ_error())
            }
        })
        .await;

        match result {
            Err(PersistenceError::OccConflictExhausted { attempts: count }) => {
                assert_eq!(count, 3);
            }
            other => panic!("expected OccConflictExhausted, got {other:?}"),
        }
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_occ_retry_non_retryable_error_fails_immediately() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<(), PersistenceError> = with_occ_retry_opts(3, 1, 5, || {
            let a = attempts_clone.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Err(make_other_error())
            }
        })
        .await;

        assert!(matches!(result, Err(PersistenceError::Sqlx(_))));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
