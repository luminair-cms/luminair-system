//! Execution mode worker for applying live database migrations (`migrate` mode).

use sqlx::PgPool;

use domain::system::SystemContext;

use super::config::CliConfig;
use super::error::CliError;
use super::report::MigrationReport;
use crate::persistence::create_connection_pool;
use crate::persistence::PersistenceError;
use crate::persistence::migration::{
    MIGRATOR, SchemaSyncResult, run_static_migrations, sync_dynamic_schemas,
};

// ---------------------------------------------------------------------------
// Public Mode Execution Entry Point
// ---------------------------------------------------------------------------

/// Executes static system migrations and dynamic schema synchronization against the live database.
///
/// # Errors
/// Returns [`CliError::Config`] if `database_url` is missing.
/// Returns [`CliError::Persistence`] if database connection, static migration, or dynamic DDL sync fails.
pub(super) async fn execute_migrate_mode(
    config: &CliConfig,
    context: &'static SystemContext,
) -> Result<MigrationReport, CliError> {
    let db_url = extract_database_url(config)?;
    let pool = connect_pool(db_url, config.max_db_connections).await?;

    tracing::info!("Applying static system migrations...");
    run_static_migrations(&pool)
        .await
        .map_err(PersistenceError::from)?;

    tracing::info!("Synchronizing dynamic document schemas...");
    let sync_result = sync_dynamic_schemas(&pool, &context.schema, config.safety_policy).await?;

    Ok(build_migration_report(sync_result))
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn extract_database_url(config: &CliConfig) -> Result<&str, CliError> {
    config
        .database_url
        .as_deref()
        .ok_or_else(|| CliError::Config("database_url is required for 'migrate' mode".to_string()))
}

async fn connect_pool(db_url: &str, max_connections: u32) -> Result<PgPool, PersistenceError> {
    create_connection_pool(db_url, max_connections).await
}

fn build_migration_report(sync_result: SchemaSyncResult) -> MigrationReport {
    MigrationReport {
        static_migrations_total: MIGRATOR.migrations.len(),
        tables_created: sync_result.tables_created,
        columns_added: sync_result.columns_added,
        indexes_created: sync_result.indexes_created,
        executed_statements: sync_result.executed_statements,
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::schema::SchemaRegistry;
    use domain::system::{LocaleId, SystemConfig, SystemConfigId, SystemContext};

    use super::*;

    fn dummy_system_context() -> &'static SystemContext {
        let registry = SchemaRegistry::new(vec![]);
        let config = SystemConfig::new(
            SystemConfigId::new(uuid::Uuid::now_v7()),
            vec![LocaleId::try_new("en").unwrap()],
            LocaleId::try_new("en").unwrap(),
        )
        .unwrap();

        Box::leak(Box::new(SystemContext::new(registry, config)))
    }

    #[tokio::test]
    async fn test_execute_migrate_mode_missing_database_url() {
        let context = dummy_system_context();
        let config = CliConfig {
            schema_dir: PathBuf::from("./config/schemas"),
            database_url: None,
            ..CliConfig::default()
        };

        let result = execute_migrate_mode(&config, context).await;
        assert!(matches!(result, Err(CliError::Config(_))));
        if let Err(CliError::Config(msg)) = result {
            assert!(msg.contains("database_url is required"));
        }
    }
}
