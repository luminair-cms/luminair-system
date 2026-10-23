//! Schema discovery, JSON parsing, validation, and SystemContext assembly.

pub mod dto;
pub mod error;
pub mod loader;

pub use error::SchemaLoaderError;
pub use loader::{
    load_system_context_from_dir, load_schemas_from_dir, parse_system_config, parse_document_type,
    parse_field_type, parse_constraints, parse_integer_size,
};
