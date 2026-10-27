//! Modular CLI subsystem for configuration validation, dry-run planning, and database migration.

mod config;
mod dry_run;
mod error;
mod migrate;
mod report;
mod runner;
mod test_config;

pub use config::{CliConfig, OutputFormat, RunMode};
pub use error::CliError;
pub use report::{
    CliOutcome, ConfigTestReport, DocumentTypeSummary, DryRunReport, MigrationReport,
    StaticMigrationStatus, StaticMigrationStepSummary,
};
pub use runner::run;
