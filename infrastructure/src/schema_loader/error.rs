//! Errors emitted during schema loading, parsing, and system context initialization.

use std::path::PathBuf;

/// Errors that can occur when reading, parsing, or validating schema definitions.
#[derive(Debug, thiserror::Error)]
pub enum SchemaLoaderError {
    #[error("I/O error reading schema file '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON deserialization error in '{path}': {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("JSON deserialization error: {source}")]
    JsonInline {
        #[source]
        source: serde_json::Error,
    },

    #[error("missing system-config.json in schema directory '{dir}'")]
    MissingSystemConfig {
        dir: PathBuf,
    },

    #[error("missing document-types directory in schema directory '{dir}'")]
    MissingDocumentTypesDir {
        dir: PathBuf,
    },

    #[error("schema file stem '{file_stem}' does not match singularName '{singular_name}'")]
    FileStemMismatch {
        file_stem: String,
        singular_name: String,
    },

    #[error("reserved SQL keyword '{keyword}' cannot be used as identifier for {context}")]
    ReservedKeyword {
        keyword: String,
        context: String,
    },

    #[error("domain validation error: {0}")]
    Domain(#[from] domain::errors::DomainError),

    #[error("schema validation error: {0}")]
    Validation(String),
}
