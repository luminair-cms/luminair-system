//! Execution mode worker for migration planning and DDL preview (`dry-run` mode).

use std::collections::HashSet;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use domain::system::SystemContext;

use super::config::CliConfig;
use super::error::CliError;
use super::report::{DryRunReport, StaticMigrationStatus, StaticMigrationStepSummary};
use crate::persistence::PersistenceError;
use crate::persistence::migration::{MIGRATOR, preview_dynamic_schemas};

// ---------------------------------------------------------------------------
// Public Mode Execution Entry Point
// ---------------------------------------------------------------------------

/// Previews database migration steps and generates DDL SQL without applying changes.
///
/// When `config.offline` is `false` and a database URL is configured, connects to the database
/// to diff against live table structures and check applied static migration versions.
/// Otherwise, plans against an empty schema offline.
///
/// # Errors
/// Returns [`CliError::Persistence`] if database connection, schema introspection, or planning fails.
pub(super) async fn execute_dry_run_mode(
    config: &CliConfig,
    context: &'static SystemContext,
) -> Result<DryRunReport, CliError> {
    match (&config.database_url, config.offline) {
        (Some(db_url), false) => execute_live_dry_run(config, context, db_url).await,
        _ => execute_offline_dry_run(config, context).await,
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

async fn execute_live_dry_run(
    config: &CliConfig,
    context: &'static SystemContext,
    db_url: &str,
) -> Result<DryRunReport, CliError> {
    let pool = connect_pool(db_url, config.max_db_connections).await?;
    let static_migrations = inspect_live_static_migrations(&pool).await?;
    let dynamic_steps =
        preview_dynamic_schemas(Some(&pool), &context.schema, config.safety_policy).await?;

    Ok(DryRunReport {
        connected_to_database: true,
        safety_policy: config.safety_policy,
        static_migrations,
        dynamic_steps,
    })
}

async fn execute_offline_dry_run(
    config: &CliConfig,
    context: &'static SystemContext,
) -> Result<DryRunReport, CliError> {
    let static_migrations = collect_offline_static_migrations();
    let dynamic_steps =
        preview_dynamic_schemas(None, &context.schema, config.safety_policy).await?;

    Ok(DryRunReport {
        connected_to_database: false,
        safety_policy: config.safety_policy,
        static_migrations,
        dynamic_steps,
    })
}

async fn connect_pool(db_url: &str, max_connections: u32) -> Result<PgPool, PersistenceError> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(db_url)
        .await
        .map_err(PersistenceError::from)
}

async fn inspect_live_static_migrations(
    pool: &PgPool,
) -> Result<Vec<StaticMigrationStepSummary>, PersistenceError> {
    let applied_versions = query_applied_static_migration_versions(pool).await?;
    Ok(map_live_static_migrations(&applied_versions))
}

async fn query_applied_static_migration_versions(
    pool: &PgPool,
) -> Result<HashSet<i64>, PersistenceError> {
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await
    .map_err(PersistenceError::from)?;

    if !table_exists {
        return Ok(HashSet::new());
    }

    let versions: Vec<i64> = sqlx::query_scalar(
        "SELECT version FROM _sqlx_migrations ORDER BY version ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(PersistenceError::from)?;

    Ok(versions.into_iter().collect())
}

fn map_live_static_migrations(applied_versions: &HashSet<i64>) -> Vec<StaticMigrationStepSummary> {
    MIGRATOR
        .migrations
        .iter()
        .map(|m| {
            let status = if applied_versions.contains(&m.version) {
                StaticMigrationStatus::Applied
            } else {
                StaticMigrationStatus::Pending
            };
            StaticMigrationStepSummary {
                version: m.version,
                description: m.description.to_string(),
                status,
            }
        })
        .collect()
}

fn collect_offline_static_migrations() -> Vec<StaticMigrationStepSummary> {
    MIGRATOR
        .migrations
        .iter()
        .map(|m| StaticMigrationStepSummary {
            version: m.version,
            description: m.description.to_string(),
            status: StaticMigrationStatus::UnverifiedOffline,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::common::DisplayName;
    use domain::schema::{
        AttributeId, DocumentKind, DocumentType, DocumentTypeId, DocumentTypeInfo,
        DocumentTypeOptions, FieldDefinition, FieldType, IntegerSize, PrimitiveType,
        SchemaRegistry,
    };
    use domain::system::{LocaleId, SystemConfig, SystemConfigId, SystemContext};

    use super::*;

    fn build_test_context() -> &'static SystemContext {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let plural_id = DocumentTypeId::try_new("articles").unwrap();
        let field = FieldDefinition {
            id: AttributeId::try_new("title").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            unique: false,
            required: true,
            constraints: Default::default(),
        };

        let article = DocumentType {
            id: type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Article").unwrap(),
                singular_name: type_id,
                plural_name: plural_id,
                description: None,
                display_fields: vec![],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: [field].into_iter().collect(),
            relations: Default::default(),
        };

        let registry = SchemaRegistry::new(vec![article]);
        let config = SystemConfig::new(
            SystemConfigId::new(uuid::Uuid::now_v7()),
            vec![LocaleId::try_new("en").unwrap()],
            LocaleId::try_new("en").unwrap(),
        )
        .unwrap();

        Box::leak(Box::new(SystemContext::new(registry, config)))
    }

    #[tokio::test]
    async fn test_execute_dry_run_offline_produces_unverified_report() {
        let context = build_test_context();
        let config = CliConfig {
            schema_dir: PathBuf::from("./config/schemas"),
            database_url: None,
            offline: true,
            ..CliConfig::default()
        };

        let report = execute_dry_run_mode(&config, context).await.unwrap();

        assert!(!report.connected_to_database);
        assert!(!report.static_migrations.is_empty());
        for migration in &report.static_migrations {
            assert_eq!(migration.status, StaticMigrationStatus::UnverifiedOffline);
        }

        // Must contain planned dynamic step for creating "articles" table
        assert!(!report.dynamic_steps.is_empty());
        let table_step = report
            .dynamic_steps
            .iter()
            .find(|s| s.operation.starts_with("CREATE TABLE \"articles\""));
        assert!(table_step.is_some());
    }

    #[test]
    fn test_map_live_static_migrations_applied_and_pending() {
        let first_version = MIGRATOR.migrations[0].version;
        let mut applied = HashSet::new();
        applied.insert(first_version);

        let mapped = map_live_static_migrations(&applied);
        assert_eq!(mapped.len(), MIGRATOR.migrations.len());
        assert_eq!(mapped[0].status, StaticMigrationStatus::Applied);

        if mapped.len() > 1 {
            assert_eq!(mapped[1].status, StaticMigrationStatus::Pending);
        }
    }
}
