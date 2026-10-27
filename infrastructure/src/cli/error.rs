//! Error types for the CLI subsystem.

use std::path::PathBuf;

use thiserror::Error;

use crate::persistence::PersistenceError;
use crate::schema_loader::SchemaLoaderError;

// ---------------------------------------------------------------------------
// Error Definitions
// ---------------------------------------------------------------------------

/// Errors arising during CLI configuration parsing, schema validation, or migration execution.
#[derive(Debug, Error)]
pub enum CliError {
    /// Error parsing command line arguments, environment variables, or config files.
    #[error("configuration error: {0}")]
    Config(String),

    /// Error reading or accessing a file from the filesystem.
    #[error("failed to access file '{path}': {source}")]
    Io {
        /// Target file path that caused the I/O failure.
        path: PathBuf,
        /// Underlying I/O error source.
        #[source]
        source: std::io::Error,
    },

    /// Error deserializing a JSON configuration file.
    #[error("failed to parse JSON config '{path}': {source}")]
    JsonConfig {
        /// Target JSON config file path.
        path: PathBuf,
        /// Underlying JSON parsing error source.
        #[source]
        source: serde_json::Error,
    },

    /// Error serializing a CLI execution report to JSON.
    #[error("report serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Error during schema discovery, JSON loading, or graph validation.
    #[error("schema validation failed: {0}")]
    SchemaLoader(#[from] SchemaLoaderError),

    /// Error during database connection, static migration, or dynamic DDL sync.
    #[error("persistence migration failed: {0}")]
    Persistence(#[from] PersistenceError),
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_error_display() {
        let err = CliError::Config("missing database_url".to_string());
        assert_eq!(err.to_string(), "configuration error: missing database_url");
    }

    #[test]
    fn test_io_error_display() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = CliError::Io {
            path: PathBuf::from("config.json"),
            source: io_err,
        };
        assert!(err.to_string().contains("failed to access file 'config.json'"));
    }

    #[test]
    fn test_json_config_error_display() {
        let json_err = serde_json::from_str::<serde_json::Value>("{invalid}").unwrap_err();
        let err = CliError::JsonConfig {
            path: PathBuf::from("config.json"),
            source: json_err,
        };
        assert!(err
            .to_string()
            .contains("failed to parse JSON config 'config.json'"));
    }

    #[test]
    fn test_serialization_error_from() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let err = CliError::from(json_err);
        assert!(err.to_string().contains("report serialization error"));
    }

    #[test]
    fn test_persistence_error_from() {
        let persistence_err = PersistenceError::Validation("invalid state".to_string());
        let err = CliError::from(persistence_err);
        assert!(err.to_string().contains("persistence migration failed"));
    }
}
