//! Dynamic schema migration pipeline: AST modeling, introspection, diffing, planning, and typed DDL execution.

mod builder;
mod ddl;
mod diff;
mod executor;
mod introspector;
mod model;
mod planner;
mod step;
mod sync;

pub use step::SafetyPolicy;
pub use sync::{PlannedMigrationStep, SchemaSyncResult, sync_dynamic_schemas};

#[cfg(test)]
pub(crate) use model::SqlColumnType;
pub(crate) use sync::preview_dynamic_schemas;
