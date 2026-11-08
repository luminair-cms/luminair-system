//! Schema discovery, JSON parsing, validation, and `SystemContext` assembly.

pub(crate) mod dto;
pub(crate) mod error;
pub(crate) mod loader;

pub use error::SchemaLoaderError;
pub use loader::{load_schemas_from_dir, load_system_context_from_dir};
