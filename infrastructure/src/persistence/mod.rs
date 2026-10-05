//! Database persistence adapters, schema migrations, and query generation.

pub mod migration;


pub use migration::{run_static_migrations};
