//! Modular CLI subsystem for configuration validation, dry-run planning, and database migration.

pub mod config;
pub mod dry_run;
pub mod error;
pub mod migrate;
pub mod report;
pub mod runner;
pub mod validate;

pub use config::{CliConfig, OutputFormat, RunMode};
pub use error::CliError;
pub use report::{
    CliOutcome, DocumentTypeSummary, DryRunReport, MigrationReport, StaticMigrationStatus,
    StaticMigrationStepSummary, ValidationReport,
};
pub use runner::run;
