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

pub(crate) use model::SqlColumnType;
pub use step::SafetyPolicy;
pub use sync::{
    SchemaSyncResult, generate_ddl_for_registry, sync_dynamic_schemas,
    sync_dynamic_schemas_from_dir,
};
