//! Modular CLI subsystem for configuration validation, dry-run planning, database migration, and HTTP service daemon.

mod config;
mod dry_run;
mod error;
mod migrate;
mod report;
mod runner;
mod service;
mod validate;

pub use config::{CliConfig, OutputFormat, RunMode};
pub use error::CliError;
pub use report::{CliOutcome, DocumentTypeSummary, ServiceReport, StaticMigrationStatus};
pub use runner::run;
