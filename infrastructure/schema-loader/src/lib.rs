//! Schema discovery, JSON parsing, validation, and `SystemContext` assembly.

pub mod dto;
pub mod error;
pub mod keywords;
pub mod loader;

pub use error::SchemaLoaderError;
pub use keywords::is_reserved_sql_keyword;
pub use loader::{load_schemas_from_dir, load_system_context_from_dir};
