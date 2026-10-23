//! Database persistence adapters, schema migrations, and query generation.

pub mod migration;
pub mod naming;

pub use migration::run_static_migrations;
