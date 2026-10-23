//! Schema discovery, JSON parsing, validation, and SystemContext assembly.

pub mod dto;
pub mod error;
pub mod loader;
pub mod sanitize;

pub use error::SchemaLoaderError;
pub use loader::{
    load_schemas_from_dir, load_system_context_from_dir, parse_constraints, parse_document_type,
    parse_field_type, parse_system_config,
};
pub use sanitize::{is_reserved_sql_keyword, kebab_to_snake, RESERVED_SQL_KEYWORDS};
