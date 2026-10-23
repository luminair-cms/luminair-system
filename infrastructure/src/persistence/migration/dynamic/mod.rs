//! Dynamic schema migration pipeline: AST modeling, introspection, diffing, planning, and typed DDL execution.

pub mod builder;
pub mod diff;
pub mod executor;
pub mod introspector;
pub mod model;
pub mod planner;
pub mod sync;

pub use builder::{build_desired_schema, build_document_type_tables};
pub use diff::{compute_diff, DiffError, MigrationStep, SafetyPolicy};
pub use executor::{build_column_def, execute_migration_plan, step_to_sql, ExecutionSummary};
pub use introspector::{introspect_database_schema, SYSTEM_TABLES};
pub use model::{
    ColumnDefinition, DatabaseSchema, ForeignKeyAction, ForeignKeyDefinition, IndexDefinition,
    SqlColumnType, TableDefinition, TableKind,
};
pub use planner::{plan_migrations, MigrationPlan};
pub use sync::{
    generate_ddl_for_document_type, generate_ddl_for_registry, sync_dynamic_schemas,
    sync_dynamic_schemas_from_dir, SchemaSyncResult,
};
