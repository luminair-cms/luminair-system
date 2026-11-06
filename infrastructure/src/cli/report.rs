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

/// Outcome report produced by the offline `validate` run mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
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
    /// Topologically ordered dynamic DDL migration steps executed against the database.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executed_steps: Vec<PlannedMigrationStep>,
    /// Exact SQL statements executed against the database.
    pub executed_statements: Vec<String>,
}

/// Outcome report produced by the `service` execution mode upon termination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReport {
    /// Bind address the HTTP server was listening on (e.g. "0.0.0.0:8080").
    pub bind_address: String,
    /// Termination message or status description.
    pub status: String,
}

/// Top-level outcome produced by any CLI execution mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum CliOutcome {
    /// Result of offline configuration and schema validation.
    #[serde(alias = "test")]
    Validate(ValidationReport),
    /// Result of dry-run migration planning and DDL preview.
    DryRun(DryRunReport),
    /// Result of live database migration execution.
    Migrate(MigrationReport),
    /// Result of HTTP server daemon execution.
    #[serde(alias = "service")]
    Serve(ServiceReport),
}

/// Scanner state for splitting top-level comma-separated SQL clauses.
#[derive(Default)]
struct SqlScanState {
    depth: usize,
    in_single_quote: bool,
    in_double_quote: bool,
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

impl SqlScanState {
    /// Advances the quote/parenthesis scanner by one character and returns `true`
    /// when `ch` is a top-level comma outside quotes and nested parentheses.
    fn is_top_level_comma(&mut self, ch: char) -> bool {
        match ch {
            '\'' if !self.in_double_quote => self.in_single_quote = !self.in_single_quote,
            '"' if !self.in_single_quote => self.in_double_quote = !self.in_double_quote,
            '(' if !self.in_single_quote && !self.in_double_quote => self.depth += 1,
            ')' if !self.in_single_quote && !self.in_double_quote => {
                self.depth = self.depth.saturating_sub(1);
            }
            ',' if !self.in_single_quote && !self.in_double_quote && self.depth == 0 => {
                return true;
            }
            _ => {}
        }
        false
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

impl fmt::Display for ValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Configuration & Schema Validation Report ===")?;
        writeln!(f, "Schema Directory:        {}", self.schema_dir)?;
        writeln!(
            f,
            "Database URL Configured: {}",
            if self.database_url_configured {
                "Yes"
            } else {
                "No"
            }
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
                doc.id,
                doc.kind,
                doc.table_name,
                doc.draft_and_publish,
                doc.field_count,
                doc.relation_count
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
            if self.connected_to_database {
                "Yes"
            } else {
                "No (Offline)"
            }
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
            write_dynamic_step(f, step)?;
        }
        Ok(())
    }
}

impl fmt::Display for MigrationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Migration Execution Report ===")?;
        writeln!(
            f,
            "Static Migrations Total: {}",
            self.static_migrations_total
        )?;
        writeln!(f, "Tables Created:          {}", self.tables_created)?;
        writeln!(f, "Columns Added:           {}", self.columns_added)?;
        writeln!(f, "Indexes Created:         {}", self.indexes_created)?;
        write_executed_migration_steps(f, &self.executed_steps, &self.executed_statements)?;
        write!(f, "Status: Completed successfully")
    }
}

impl fmt::Display for ServiceReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Luminair HTTP Service Report ===")?;
        writeln!(f, "Bind Address: {}", self.bind_address)?;
        write!(f, "Status:       {}", self.status)
    }
}

impl fmt::Display for CliOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validate(report) => write!(f, "{report}"),
            Self::DryRun(report) => write!(f, "{report}"),
            Self::Migrate(report) => write!(f, "{report}"),
            Self::Serve(report) => write!(f, "{report}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

/// Writes a single planned or executed migration step header followed by its indented, formatted DDL SQL.
fn write_dynamic_step(f: &mut fmt::Formatter<'_>, step: &PlannedMigrationStep) -> fmt::Result {
    writeln!(f, "  {}. {}", step.step_number, step.operation)?;
    write_indented_formatted_sql(f, &step.sql)
}

/// Writes executed migration steps (or fallback raw statements) with formatted multi-line DDL SQL.
fn write_executed_migration_steps(
    f: &mut fmt::Formatter<'_>,
    executed_steps: &[PlannedMigrationStep],
    executed_statements: &[String],
) -> fmt::Result {
    if !executed_steps.is_empty() {
        writeln!(f, "Executed Statements ({}):", executed_steps.len())?;
        for step in executed_steps {
            write_dynamic_step(f, step)?;
        }
        return Ok(());
    }

    writeln!(f, "Executed Statements ({}):", executed_statements.len())?;
    for (idx, stmt) in executed_statements.iter().enumerate() {
        write_numbered_sql_statement(f, idx + 1, stmt)?;
    }
    Ok(())
}

/// Writes a numbered SQL statement with multi-line DDL formatting when no step summary is present.
fn write_numbered_sql_statement(
    f: &mut fmt::Formatter<'_>,
    step_number: usize,
    sql: &str,
) -> fmt::Result {
    let formatted_sql = format_ddl_sql(sql);
    let mut lines = formatted_sql.lines();
    if let Some(first_line) = lines.next() {
        writeln!(f, "  {step_number}. {first_line}")?;
        for line in lines {
            writeln!(f, "     {line}")?;
        }
    }
    Ok(())
}

/// Formats `sql` and writes each line indented by 5 spaces under a step header.
fn write_indented_formatted_sql(f: &mut fmt::Formatter<'_>, sql: &str) -> fmt::Result {
    let formatted_sql = format_ddl_sql(sql);
    for line in formatted_sql.lines() {
        writeln!(f, "     {line}")?;
    }
    Ok(())
}

/// Formats a DDL SQL statement for human-readable multi-line text output.
///
/// Expands `CREATE TABLE` column and constraint definitions onto individual
/// indented lines while preserving nested parentheses (e.g., `DECIMAL(10, 2)`,
/// `PRIMARY KEY ("source_id", "target_id")`, `FOREIGN KEY (...)`). Non-table
/// DDL statements (`CREATE INDEX`, `ALTER TABLE`, etc.) are returned as a
/// single semicolon-terminated line.
fn format_ddl_sql(sql: &str) -> String {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return String::new();
    }

    try_format_create_table(trimmed).unwrap_or_else(|| format!("{trimmed};"))
}

/// Attempts to format a `CREATE TABLE ... (...)` statement across multiple indented lines.
fn try_format_create_table(trimmed: &str) -> Option<String> {
    let is_create_table = trimmed
        .get(..12)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("CREATE TABLE"));
    if !is_create_table || !trimmed.ends_with(')') {
        return None;
    }

    let open_paren = trimmed.find('(')?;
    let header = trimmed[..=open_paren].trim_end();
    let inner = &trimmed[open_paren + 1..trimmed.len() - 1];
    let items = split_top_level_sql_items(inner);
    if items.is_empty() {
        return None;
    }

    let body = items.join(",\n  ");
    Some(format!("{header}\n  {body}\n);"))
}

/// Splits a comma-separated SQL clause body into top-level items, ignoring
/// commas enclosed inside parentheses or single/double quotes.
fn split_top_level_sql_items(inner: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut state = SqlScanState::default();
    let mut segment_start = 0;

    for (idx, ch) in inner.char_indices() {
        if state.is_top_level_comma(ch) {
            push_trimmed(&mut items, &inner[segment_start..idx]);
            segment_start = idx + 1;
        }
    }

    push_trimmed(&mut items, &inner[segment_start..]);
    items
}

/// Pushes a trimmed SQL clause slice into `items` if non-empty.
fn push_trimmed<'a>(items: &mut Vec<&'a str>, segment: &'a str) {
    let piece = segment.trim();
    if !piece.is_empty() {
        items.push(piece);
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_validate_report() -> ValidationReport {
        ValidationReport {
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
            dynamic_steps: vec![
                PlannedMigrationStep {
                    step_number: 1,
                    operation: "CREATE TABLE \"articles\" (2 columns, 0 foreign keys)".to_string(),
                    sql: "CREATE TABLE IF NOT EXISTS \"articles\" (\"id\" uuid NOT NULL PRIMARY KEY, \"title\" varchar(255) NOT NULL);".to_string(),
                },
                PlannedMigrationStep {
                    step_number: 2,
                    operation: "CREATE INDEX \"idx_articles_title\" ON \"articles\"".to_string(),
                    sql: "CREATE INDEX IF NOT EXISTS \"idx_articles_title\" ON \"articles\" (\"title\");".to_string(),
                },
            ],
        }
    }

    fn sample_migrate_report() -> MigrationReport {
        let sql = "CREATE TABLE IF NOT EXISTS \"articles\" (\"id\" uuid NOT NULL PRIMARY KEY, \"title\" varchar(255) NOT NULL);".to_string();
        MigrationReport {
            static_migrations_total: 1,
            tables_created: 1,
            columns_added: 0,
            indexes_created: 0,
            executed_steps: vec![PlannedMigrationStep {
                step_number: 1,
                operation: "CREATE TABLE \"articles\" (2 columns, 0 foreign keys)".to_string(),
                sql: sql.clone(),
            }],
            executed_statements: vec![sql],
        }
    }

    #[test]
    fn test_render_validate_outcome_text_and_json() {
        let outcome = CliOutcome::Validate(sample_validate_report());

        let text = outcome.render(OutputFormat::Text).unwrap();
        assert!(text.contains("=== Configuration & Schema Validation Report ==="));
        assert!(text.contains("article"));
        assert!(text.contains("Status: OK"));

        let json = outcome.render(OutputFormat::Json).unwrap();
        assert!(json.contains("\"mode\": \"validate\""));
        assert!(json.contains("\"schemaDir\": \"./config/schemas\""));
        assert!(json.contains("\"defaultLocale\": \"en\""));
    }

    #[test]
    fn test_render_dry_run_outcome_text_and_json() {
        let outcome = CliOutcome::DryRun(sample_dry_run_report());

        let text = outcome
            .render(OutputFormat::Text)
            .expect("dry-run text rendering must succeed");
        assert!(text.contains("=== Dry-Run Migration Plan Report ==="));
        assert!(text.contains("Database Connected: Yes"));
        assert!(!text.contains("-- Generated DDL SQL:"));

        let step1_header_pos = text
            .find("  1. CREATE TABLE \"articles\" (2 columns, 0 foreign keys)")
            .expect("step 1 header must exist");
        let step1_sql_pos = text
            .find("     CREATE TABLE IF NOT EXISTS \"articles\" (\n       \"id\" uuid NOT NULL PRIMARY KEY,\n       \"title\" varchar(255) NOT NULL\n     );")
            .expect("step 1 formatted multi-line SQL must exist");
        let step2_header_pos = text
            .find("  2. CREATE INDEX \"idx_articles_title\" ON \"articles\"")
            .expect("step 2 header must exist");
        let step2_sql_pos = text
            .find("     CREATE INDEX IF NOT EXISTS \"idx_articles_title\" ON \"articles\" (\"title\");")
            .expect("step 2 SQL must exist");

        assert!(step1_header_pos < step1_sql_pos);
        assert!(step1_sql_pos < step2_header_pos);
        assert!(step2_header_pos < step2_sql_pos);

        let json = outcome
            .render(OutputFormat::Json)
            .expect("dry-run JSON rendering must succeed");
        assert!(json.contains("\"mode\": \"dryRun\""));
        assert!(json.contains("\"connectedToDatabase\": true"));
    }

    #[test]
    fn test_format_ddl_sql_create_table_and_index() {
        let create_table_sql = concat!(
            "CREATE TABLE IF NOT EXISTS \"articles_tags_links\" (",
            "\"source_id\" uuid NOT NULL, ",
            "\"target_id\" uuid NOT NULL, ",
            "\"weight\" DECIMAL(10, 2) NOT NULL DEFAULT 0.00, ",
            "PRIMARY KEY (\"source_id\", \"target_id\"), ",
            "FOREIGN KEY (\"source_id\") REFERENCES \"articles\" (\"id\") ON DELETE CASCADE",
            ");"
        );

        let formatted = format_ddl_sql(create_table_sql);
        let expected = concat!(
            "CREATE TABLE IF NOT EXISTS \"articles_tags_links\" (\n",
            "  \"source_id\" uuid NOT NULL,\n",
            "  \"target_id\" uuid NOT NULL,\n",
            "  \"weight\" DECIMAL(10, 2) NOT NULL DEFAULT 0.00,\n",
            "  PRIMARY KEY (\"source_id\", \"target_id\"),\n",
            "  FOREIGN KEY (\"source_id\") REFERENCES \"articles\" (\"id\") ON DELETE CASCADE\n",
            ");"
        );
        assert_eq!(formatted, expected);

        let create_index_sql =
            "CREATE UNIQUE INDEX IF NOT EXISTS \"idx_articles_slug\" ON \"articles\" (\"slug\");";
        assert_eq!(format_ddl_sql(create_index_sql), create_index_sql);
    }

    #[test]
    fn test_render_migrate_outcome_text_and_json() {
        let outcome = CliOutcome::Migrate(sample_migrate_report());

        let text = outcome
            .render(OutputFormat::Text)
            .expect("migrate text rendering must succeed");
        assert!(text.contains("=== Migration Execution Report ==="));
        assert!(text.contains("Tables Created:          1"));
        assert!(text.contains("  1. CREATE TABLE \"articles\" (2 columns, 0 foreign keys)"));
        assert!(text.contains(
            "     CREATE TABLE IF NOT EXISTS \"articles\" (\n       \"id\" uuid NOT NULL PRIMARY KEY,\n       \"title\" varchar(255) NOT NULL\n     );"
        ));
        assert!(text.contains("Status: Completed successfully"));

        let json = outcome
            .render(OutputFormat::Json)
            .expect("migrate JSON rendering must succeed");
        assert!(json.contains("\"mode\": \"migrate\""));
        assert!(json.contains("\"tablesCreated\": 1"));
    }

    #[test]
    fn test_write_report_to_file() {
        let outcome = CliOutcome::Validate(sample_validate_report());
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
