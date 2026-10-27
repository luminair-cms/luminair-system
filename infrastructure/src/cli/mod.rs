//! Modular CLI subsystem for configuration validation, dry-run planning, database migration, and HTTP service daemon.

mod config;
mod dry_run;
mod error;
mod migrate;
mod report;
mod runner;
mod service;
mod test_config;

pub use config::{CliConfig, OutputFormat, RunMode, ServiceTokenConfig};
pub use error::CliError;
pub use report::{
    CliOutcome, ConfigTestReport, DocumentTypeSummary, DryRunReport, MigrationReport,
    ServiceReport, StaticMigrationStatus, StaticMigrationStepSummary,
};
pub use runner::run;
pub use service::execute_service_mode;
