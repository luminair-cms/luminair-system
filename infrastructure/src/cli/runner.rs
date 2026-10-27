//! Top-level CLI execution orchestrator dispatching commands to mode workers.

use domain::system::SystemContext;

use super::config::{CliConfig, RunMode};
use super::dry_run::execute_dry_run_mode;
use super::error::CliError;
use super::migrate::execute_migrate_mode;
use super::report::CliOutcome;
use super::service::execute_service_mode;
use super::test_config::execute_test_mode;
use crate::schema_loader::load_system_context_from_dir;

// ---------------------------------------------------------------------------
// Public Orchestrator Entry Point
// ---------------------------------------------------------------------------

/// Orchestrates CLI execution according to the provided [`CliConfig`].
///
/// 1. Validates configuration constraints and parameters.
/// 2. Loads and validates all schema definitions into [`SystemContext`].
/// 3. Dispatches execution to the corresponding mode worker.
///
/// # Errors
/// Returns [`CliError`] if validation, schema loading, or mode execution fails.
pub async fn run(config: &CliConfig) -> Result<CliOutcome, CliError> {
    config.validate()?;
    let context = load_system_context_from_dir(&config.schema_dir)?;
    dispatch_mode(config, context).await
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

async fn dispatch_mode(
    config: &CliConfig,
    context: &'static SystemContext,
) -> Result<CliOutcome, CliError> {
    match config.mode {
        RunMode::Test => Ok(CliOutcome::Test(execute_test_mode(config, context))),
        RunMode::DryRun => Ok(CliOutcome::DryRun(
            execute_dry_run_mode(config, context).await?,
        )),
        RunMode::Migrate => Ok(CliOutcome::Migrate(
            execute_migrate_mode(config, context).await?,
        )),
        RunMode::Service => Ok(execute_service_mode(config, context).await?),
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn write_test_schemas_dir() -> PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("luminair_runner_test_{}", uuid::Uuid::now_v7()));
        let doc_types_dir = temp_dir.join("document-types");
        std::fs::create_dir_all(&doc_types_dir).unwrap();

        let system_config_json = r#"{
            "defaultLocale": "en",
            "availableLocales": ["en", "uk"]
        }"#;
        std::fs::write(temp_dir.join("system-config.json"), system_config_json).unwrap();

        let article_json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "post",
                "pluralName": "posts",
                "displayName": "Post"
            },
            "options": {
                "draftAndPublish": true
            },
            "attributes": {
                "title": {
                    "type": "text",
                    "required": true
                }
            }
        }"#;
        std::fs::write(doc_types_dir.join("post.json"), article_json).unwrap();

        temp_dir
    }

    #[tokio::test]
    async fn test_run_rejects_invalid_config() {
        let config = CliConfig {
            max_db_connections: 0,
            ..CliConfig::default()
        };
        let result = run(&config).await;
        assert!(matches!(result, Err(CliError::Config(_))));
    }

    #[tokio::test]
    async fn test_run_test_mode_success() {
        let schema_dir = write_test_schemas_dir();
        let config = CliConfig {
            mode: RunMode::Test,
            schema_dir: schema_dir.clone(),
            ..CliConfig::default()
        };

        let result = run(&config).await.expect("run should succeed in test mode");
        match result {
            CliOutcome::Test(report) => {
                assert_eq!(report.default_locale, "en");
                assert_eq!(report.document_types.len(), 1);
                assert_eq!(report.document_types[0].id, "post");
            }
            _ => panic!("expected CliOutcome::Test"),
        }

        let _ = std::fs::remove_dir_all(&schema_dir);
    }

    #[tokio::test]
    async fn test_run_dry_run_offline_success() {
        let schema_dir = write_test_schemas_dir();
        let config = CliConfig {
            mode: RunMode::DryRun,
            schema_dir: schema_dir.clone(),
            offline: true,
            ..CliConfig::default()
        };

        let result = run(&config).await.expect("run should succeed in offline dry-run");
        match result {
            CliOutcome::DryRun(report) => {
                assert!(!report.connected_to_database);
                assert!(!report.dynamic_steps.is_empty());
            }
            _ => panic!("expected CliOutcome::DryRun"),
        }

        let _ = std::fs::remove_dir_all(&schema_dir);
    }
}
