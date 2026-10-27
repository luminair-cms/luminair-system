//! CLI execution report structures, display formatters, and serialization helpers.

use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::config::OutputFormat;
use super::error::CliError;
use crate::persistence::migration::{PlannedMigrationStep, SafetyPolicy};

// ---------------------------------------------------------------------------
// Report Data Structures
// ---------------------------------------------------------------------------

/// Metadata summary of a single document type discovered in schemas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentTypeSummary {
    /// Schema API identifier (e.g. "article").
    pub id: String,
    /// Document kind (e.g. "collection", "single").
    pub kind: String,
    /// Relational target table name (e.g. "articles").
    pub table_name: String,
    /// Whether the document uses draft and publish workflows.
    pub draft_and_publish: bool,
    /// Number of declared scalar attribute fields.
    pub field_count: usize,
    /// Number of declared relational connections.
    pub relation_count: usize,
}

/// Outcome report produced by the offline `test` run mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTestReport {
    /// Schema directory that was discovered and parsed.
    pub schema_dir: String,
    /// Default locale configured for the system.
    pub default_locale: String,
    /// List of all supported locale identifiers.
    pub available_locales: Vec<String>,
    /// Summaries of all loaded document types.
    pub document_types: Vec<DocumentTypeSummary>,
    /// Whether a valid database connection URL was configured.
    pub database_url_configured: bool,
}

/// Execution status of a static migration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StaticMigrationStatus {
    /// Migration has not yet been applied to the database.
    Pending,
    /// Migration has been applied and recorded in sqlx migrations table.
    Applied,
    /// Running offline without live database connectivity to verify version.
    UnverifiedOffline,
}

/// Summary of a static embedded SQL migration step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticMigrationStepSummary {
    /// Numeric timestamp version of the migration.
    pub version: i64,
    /// Description extracted from migration filename.
    pub description: String,
    /// Application status of the migration.
    pub status: StaticMigrationStatus,
}

/// Outcome report produced by the `dry-run` mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunReport {
    /// Whether a live database connection was established for schema diffing.
    pub connected_to_database: bool,
    /// Active safety policy governing dynamic plan generation.
    pub safety_policy: SafetyPolicy,
    /// Static system migrations planned or inspected.
    pub static_migrations: Vec<StaticMigrationStepSummary>,
    /// Topologically ordered dynamic DDL migration steps.
    pub dynamic_steps: Vec<PlannedMigrationStep>,
}

/// Outcome report produced by the `migrate` execution mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    /// Count of static system migrations executed.
    pub static_migrations_total: usize,
    /// Count of new relational tables created.
    pub tables_created: usize,
    /// Count of columns added to existing tables.
    pub columns_added: usize,
    /// Count of secondary indexes created.
    pub indexes_created: usize,
    /// Exact SQL statements executed against the database.
    pub executed_statements: Vec<String>,
}

/// Top-level outcome produced by any CLI execution mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum CliOutcome {
    /// Result of offline configuration and schema validation.
    Test(ConfigTestReport),
    /// Result of dry-run migration planning and DDL preview.
    DryRun(DryRunReport),
    /// Result of live database migration execution.
    Migrate(MigrationReport),
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl CliOutcome {
    /// Renders the outcome into the requested output format.
    pub fn render(&self, format: OutputFormat) -> Result<String, CliError> {
        match format {
            OutputFormat::Json => {
                serde_json::to_string_pretty(self).map_err(CliError::Serialization)
            }
            OutputFormat::Text => Ok(self.to_string()),
        }
    }

    /// Renders the report and optionally writes the result to a destination file.
    pub fn write_report(
        &self,
        format: OutputFormat,
        output_path: Option<&Path>,
    ) -> Result<String, CliError> {
        let rendered = self.render(format)?;
        if let Some(path) = output_path {
            std::fs::write(path, &rendered).map_err(|source| CliError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        }
        Ok(rendered)
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl fmt::Display for StaticMigrationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Applied => write!(f, "Applied"),
            Self::UnverifiedOffline => write!(f, "Unverified (offline)"),
        }
    }
}

impl fmt::Display for ConfigTestReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Configuration & Schema Test Report ===")?;
        writeln!(f, "Schema Directory:        {}", self.schema_dir)?;
        writeln!(
            f,
            "Database URL Configured: {}",
            if self.database_url_configured { "Yes" } else { "No" }
        )?;
        writeln!(f, "Default Locale:          {}", self.default_locale)?;
        writeln!(
            f,
            "Available Locales:       {}",
            self.available_locales.join(", ")
        )?;
        writeln!(f, "Document Types ({}):", self.document_types.len())?;
        for doc in &self.document_types {
            writeln!(
                f,
                "  - {} (kind: {}, table: '{}', draft/publish: {}, {} fields, {} relations)",
                doc.id, doc.kind, doc.table_name, doc.draft_and_publish, doc.field_count, doc.relation_count
            )?;
        }
        write!(f, "Status: OK (all schema definitions valid)")
    }
}

impl fmt::Display for DryRunReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Dry-Run Migration Plan Report ===")?;
        writeln!(
            f,
            "Database Connected: {}",
            if self.connected_to_database { "Yes" } else { "No (Offline)" }
        )?;
        writeln!(f, "Safety Policy:      {:?}", self.safety_policy)?;
        writeln!(f, "Static Migrations ({}):", self.static_migrations.len())?;
        for migration in &self.static_migrations {
            writeln!(
                f,
                "  - [{}] {}: {}",
                migration.status, migration.version, migration.description
            )?;
        }
        writeln!(f, "Dynamic Steps ({}):", self.dynamic_steps.len())?;
        for step in &self.dynamic_steps {
            writeln!(f, "  {}. {}", step.step_number, step.operation)?;
        }
        writeln!(f, "\n-- Generated DDL SQL:")?;
        for step in &self.dynamic_steps {
            writeln!(f, "{}", step.sql)?;
        }
        Ok(())
    }
}

impl fmt::Display for MigrationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Migration Execution Report ===")?;
        writeln!(f, "Static Migrations Total: {}", self.static_migrations_total)?;
        writeln!(f, "Tables Created:          {}", self.tables_created)?;
        writeln!(f, "Columns Added:           {}", self.columns_added)?;
        writeln!(f, "Indexes Created:         {}", self.indexes_created)?;
        writeln!(f, "Executed Statements ({}):", self.executed_statements.len())?;
        for (i, stmt) in self.executed_statements.iter().enumerate() {
            writeln!(f, "  {}. {}", i + 1, stmt)?;
        }
        write!(f, "Status: Completed successfully")
    }
}

impl fmt::Display for CliOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Test(report) => write!(f, "{report}"),
            Self::DryRun(report) => write!(f, "{report}"),
            Self::Migrate(report) => write!(f, "{report}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_test_report() -> ConfigTestReport {
        ConfigTestReport {
            schema_dir: "./config/schemas".to_string(),
            default_locale: "en".to_string(),
            available_locales: vec!["en".to_string(), "fr".to_string()],
            document_types: vec![DocumentTypeSummary {
                id: "article".to_string(),
                kind: "collection".to_string(),
                table_name: "articles".to_string(),
                draft_and_publish: true,
                field_count: 5,
                relation_count: 2,
            }],
            database_url_configured: true,
        }
    }

    fn sample_dry_run_report() -> DryRunReport {
        DryRunReport {
            connected_to_database: true,
            safety_policy: SafetyPolicy::AdditiveOnly,
            static_migrations: vec![StaticMigrationStepSummary {
                version: 20261010000001,
                description: "create_document_drafts".to_string(),
                status: StaticMigrationStatus::Applied,
            }],
            dynamic_steps: vec![PlannedMigrationStep {
                step_number: 1,
                operation: "CREATE TABLE \"articles\" (1 columns, 0 foreign keys)".to_string(),
                sql: "CREATE TABLE IF NOT EXISTS articles (id UUID PRIMARY KEY);".to_string(),
            }],
        }
    }

    fn sample_migrate_report() -> MigrationReport {
        MigrationReport {
            static_migrations_total: 1,
            tables_created: 1,
            columns_added: 0,
            indexes_created: 0,
            executed_statements: vec![
                "CREATE TABLE IF NOT EXISTS articles (id UUID PRIMARY KEY);".to_string(),
            ],
        }
    }

    #[test]
    fn test_render_config_test_outcome_text_and_json() {
        let outcome = CliOutcome::Test(sample_test_report());

        let text = outcome.render(OutputFormat::Text).unwrap();
        assert!(text.contains("=== Configuration & Schema Test Report ==="));
        assert!(text.contains("article"));
        assert!(text.contains("Status: OK"));

        let json = outcome.render(OutputFormat::Json).unwrap();
        assert!(json.contains("\"mode\": \"test\""));
        assert!(json.contains("\"schemaDir\": \"./config/schemas\""));
        assert!(json.contains("\"defaultLocale\": \"en\""));
    }

    #[test]
    fn test_render_dry_run_outcome_text_and_json() {
        let outcome = CliOutcome::DryRun(sample_dry_run_report());

        let text = outcome.render(OutputFormat::Text).unwrap();
        assert!(text.contains("=== Dry-Run Migration Plan Report ==="));
        assert!(text.contains("Database Connected: Yes"));
        assert!(text.contains("CREATE TABLE IF NOT EXISTS articles"));
        assert!(text.contains("-- Generated DDL SQL:"));

        let json = outcome.render(OutputFormat::Json).unwrap();
        assert!(json.contains("\"mode\": \"dryRun\""));
        assert!(json.contains("\"connectedToDatabase\": true"));
    }

    #[test]
    fn test_render_migrate_outcome_text_and_json() {
        let outcome = CliOutcome::Migrate(sample_migrate_report());

        let text = outcome.render(OutputFormat::Text).unwrap();
        assert!(text.contains("=== Migration Execution Report ==="));
        assert!(text.contains("Tables Created:          1"));
        assert!(text.contains("Status: Completed successfully"));

        let json = outcome.render(OutputFormat::Json).unwrap();
        assert!(json.contains("\"mode\": \"migrate\""));
        assert!(json.contains("\"tablesCreated\": 1"));
    }

    #[test]
    fn test_write_report_to_file() {
        let outcome = CliOutcome::Test(sample_test_report());
        let temp_dir = std::env::temp_dir();
        let target_file = temp_dir.join("test_write_report.txt");

        let rendered = outcome
            .write_report(OutputFormat::Text, Some(&target_file))
            .unwrap();
        let file_contents = std::fs::read_to_string(&target_file).unwrap();
        assert_eq!(rendered, file_contents);

        let _ = std::fs::remove_file(&target_file);
    }
}
