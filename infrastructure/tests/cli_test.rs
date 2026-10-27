//! Integration tests for the Luminair CLI subsystem (`infrastructure::cli`).
//!
//! Validates `test`, `dry-run`, and `migrate` modes against prepared configuration
//! fixtures in `config/cli-config.json` and `config/schemas/`.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use infrastructure::cli::{
    CliConfig, CliError, CliOutcome, DocumentTypeSummary, OutputFormat, RunMode,
    StaticMigrationStatus, run,
};
use infrastructure::persistence::migration::PlannedMigrationStep;

// ---------------------------------------------------------------------------
// RAII Test Fixtures
// ---------------------------------------------------------------------------

/// RAII guard to safely remove temporary directories upon test completion.
struct TempDirGuard {
    path: PathBuf,
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// Integration Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cli_test_mode_with_prepared_config_succeeds() {
    let root = workspace_root();
    let config_path = root.join("config/cli-config.json");
    let mut config = CliConfig::from_json_file(&config_path)
        .expect("prepared cli-config.json must load successfully");

    config.schema_dir = root.join("config/schemas");
    assert_eq!(config.mode, RunMode::Test);

    let outcome = run(&config)
        .await
        .expect("CLI test mode execution must succeed");

    let CliOutcome::Test(report) = outcome else {
        panic!("expected CliOutcome::Test, got: {outcome:?}");
    };

    assert_eq!(report.default_locale, "en");
    assert!(report.available_locales.contains(&"en".to_string()));
    assert!(report.available_locales.contains(&"uk".to_string()));
    assert!(report.available_locales.contains(&"fr".to_string()));
    assert!(report.available_locales.contains(&"de".to_string()));
    assert_eq!(report.document_types.len(), 4);

    assert_article_document_type(&report.document_types);
    assert_category_document_type(&report.document_types);
    assert_tag_document_type(&report.document_types);
    assert_site_settings_document_type(&report.document_types);
}

#[tokio::test]
async fn test_cli_test_mode_rejects_invalid_schema() {
    let guard = create_broken_schema_dir();
    let config = CliConfig {
        mode: RunMode::Test,
        schema_dir: guard.path.clone(),
        ..CliConfig::default()
    };

    let result = run(&config).await;
    assert!(
        matches!(result, Err(CliError::SchemaLoader(_))),
        "expected Err(CliError::SchemaLoader), got: {result:?}"
    );
}

#[tokio::test]
async fn test_cli_dry_run_offline_with_prepared_config_generates_steps_and_ddl() {
    let root = workspace_root();
    let config = CliConfig {
        mode: RunMode::DryRun,
        schema_dir: root.join("config/schemas"),
        offline: true,
        ..CliConfig::default()
    };

    let outcome = run(&config)
        .await
        .expect("offline dry-run with prepared config should succeed");

    let CliOutcome::DryRun(ref report) = outcome else {
        panic!("expected CliOutcome::DryRun, got: {outcome:?}");
    };

    assert!(!report.connected_to_database);
    assert_eq!(report.static_migrations.len(), 4);
    for migration in &report.static_migrations {
        assert_eq!(migration.status, StaticMigrationStatus::UnverifiedOffline);
    }

    assert!(!report.dynamic_steps.is_empty());
    assert_dynamic_steps_contain_expected_tables(&report.dynamic_steps);
    assert_outcome_rendering_and_file_writing(&outcome);
}

#[tokio::test]
async fn test_cli_migrate_mode_without_database_url_fails_validation() {
    let root = workspace_root();
    let config = CliConfig {
        schema_dir: root.join("config/schemas"),
        mode: RunMode::Migrate,
        database_url: None,
        ..CliConfig::default()
    };

    let result = run(&config).await;
    let Err(CliError::Config(msg)) = result else {
        panic!("expected Err(CliError::Config), got: {result:?}");
    };
    assert!(
        msg.contains("database_url is required"),
        "unexpected error message: {msg}"
    );
}

#[tokio::test]
async fn test_cli_live_database_migrate_and_dry_run_when_database_url_set() {
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(val) if !val.trim().is_empty() => val,
        _ => return,
    };

    let root = workspace_root();

    // 1. Run live database migration
    let migrate_config = CliConfig {
        schema_dir: root.join("config/schemas"),
        mode: RunMode::Migrate,
        database_url: Some(db_url.clone()),
        ..CliConfig::default()
    };

    let migrate_outcome = run(&migrate_config)
        .await
        .expect("migrate against live DB must succeed");

    let CliOutcome::Migrate(migrate_report) = migrate_outcome else {
        panic!("expected CliOutcome::Migrate, got: {migrate_outcome:?}");
    };
    assert_eq!(migrate_report.static_migrations_total, 4);

    // 2. Run live online dry-run: all steps should be applied and empty
    let dry_run_config = CliConfig {
        schema_dir: root.join("config/schemas"),
        mode: RunMode::DryRun,
        database_url: Some(db_url),
        offline: false,
        ..CliConfig::default()
    };

    let dry_run_outcome = run(&dry_run_config)
        .await
        .expect("online dry-run against live DB must succeed");

    let CliOutcome::DryRun(dry_run_report) = dry_run_outcome else {
        panic!("expected CliOutcome::DryRun, got: {dry_run_outcome:?}");
    };
    assert!(dry_run_report.connected_to_database);
    for migration in &dry_run_report.static_migrations {
        assert_eq!(migration.status, StaticMigrationStatus::Applied);
    }
    assert!(
        dry_run_report.dynamic_steps.is_empty(),
        "expected zero dynamic steps after full migration, got: {:?}",
        dry_run_report.dynamic_steps
    );
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cargo manifest dir must have a parent workspace directory")
        .to_path_buf()
}

fn assert_article_document_type(document_types: &[DocumentTypeSummary]) {
    let article = document_types
        .iter()
        .find(|d| d.id == "article")
        .expect("article document type should exist");
    assert_eq!(article.table_name, "articles");
    assert_eq!(article.kind, "collection");
    assert!(article.draft_and_publish);
}

fn assert_category_document_type(document_types: &[DocumentTypeSummary]) {
    let category = document_types
        .iter()
        .find(|d| d.id == "category")
        .expect("category document type should exist");
    assert_eq!(category.table_name, "categories");
    assert_eq!(category.kind, "collection");
    assert!(!category.draft_and_publish);
}

fn assert_tag_document_type(document_types: &[DocumentTypeSummary]) {
    let tag = document_types
        .iter()
        .find(|d| d.id == "tag")
        .expect("tag document type should exist");
    assert_eq!(tag.table_name, "tags");
    assert_eq!(tag.kind, "collection");
    assert!(!tag.draft_and_publish);
}

fn assert_site_settings_document_type(document_types: &[DocumentTypeSummary]) {
    let site_settings = document_types
        .iter()
        .find(|d| d.id == "site-settings")
        .expect("site-settings document type should exist");
    assert_eq!(site_settings.table_name, "site_settings");
    assert_eq!(site_settings.kind, "singleton");
    assert!(
        site_settings.draft_and_publish,
        "site-settings schema specifies draftAndPublish: true"
    );
}

fn create_broken_schema_dir() -> TempDirGuard {
    let temp_dir = std::env::temp_dir().join(format!("luminair_broken_schema_{}", Uuid::now_v7()));
    std::fs::create_dir_all(temp_dir.join("document-types"))
        .expect("create temporary document-types dir");

    let system_config_json = r#"{
        "defaultLocale": "en",
        "availableLocales": ["en"],
        "instanceName": "Broken Schema Test Studio"
    }"#;
    std::fs::write(temp_dir.join("system-config.json"), system_config_json)
        .expect("write system-config.json");

    let broken_doc_type_json = r#"{
        "kind": "collection",
        "info": {
            "singularName": "article",
            "pluralName": "articles",
            "displayName": "Article",
            "displayFields": ["title"]
        },
        "options": {
            "draftAndPublish": false
        },
        "attributes": {
            "title": {
                "type": "text",
                "required": true
            },
            "broken-rel": {
                "relation": "hasOne",
                "target": "non-existent-target"
            }
        }
    }"#;
    std::fs::write(
        temp_dir.join("document-types/article.json"),
        broken_doc_type_json,
    )
    .expect("write broken article.json");

    TempDirGuard { path: temp_dir }
}

fn assert_dynamic_steps_contain_expected_tables(steps: &[PlannedMigrationStep]) {
    let expected_tables = [
        "articles",
        "categories",
        "tags",
        "site_settings",
        "articles_tags_links",
    ];
    for table in expected_tables {
        let found = steps.iter().any(|step| {
            step.operation.contains("CREATE TABLE") && step.operation.contains(table)
        });
        assert!(found, "expected CREATE TABLE step for table '{table}'");
    }
}

fn assert_outcome_rendering_and_file_writing(outcome: &CliOutcome) {
    let rendered_text = outcome
        .render(OutputFormat::Text)
        .expect("text rendering must succeed");
    assert!(rendered_text.contains("=== Dry-Run Migration Plan Report ==="));
    assert!(rendered_text.contains("Database Connected: No"));
    assert!(rendered_text.contains("-- Generated DDL SQL:"));
    assert!(rendered_text.contains("CREATE TABLE IF NOT EXISTS"));

    let rendered_json = outcome
        .render(OutputFormat::Json)
        .expect("JSON rendering must succeed");
    assert!(rendered_json.contains("\"mode\": \"dryRun\""));
    assert!(rendered_json.contains("\"connectedToDatabase\": false"));

    let temp_file =
        std::env::temp_dir().join(format!("luminair_dry_run_test_{}.txt", Uuid::now_v7()));
    let written = outcome
        .write_report(OutputFormat::Text, Some(&temp_file))
        .expect("file writing must succeed");
    assert_eq!(written, rendered_text);

    let read_back = std::fs::read_to_string(&temp_file).expect("reading back written report");
    assert_eq!(read_back, rendered_text);
    let _ = std::fs::remove_file(&temp_file);
}
