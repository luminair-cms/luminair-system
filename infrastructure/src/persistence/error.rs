//! Persistence error types for database queries, migrations, dynamic sync, and OCC operations.

use thiserror::Error;

/// Errors arising within the persistence infrastructure layer.
#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("database query error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("static migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("schema loader error: {0}")]
    SchemaLoader(#[from] crate::schema_loader::SchemaLoaderError),

    #[error("optimistic concurrency conflict exhausted after {attempts} attempts")]
    OccConflictExhausted { attempts: usize },

    #[error("schema synchronizer safety policy violation: {0}")]
    SafetyViolation(String),

    #[error("persistence validation error: {0}")]
    Validation(String),

    #[error("domain error: {0}")]
    Domain(#[from] domain::errors::DomainError),
}

impl From<PersistenceError> for domain::errors::DomainError {
    fn from(err: PersistenceError) -> Self {
        match err {
            PersistenceError::Domain(d) => d,
            other => domain::errors::DomainError::Storage(other.to_string()),
        }
    }
}
