//! Dynamic schema migration pipeline: AST modeling, introspection, diffing, planning, and typed DDL execution.

pub mod builder;
pub mod ddl;
pub mod diff;
pub mod executor;
pub mod introspector;
pub mod model;
pub mod planner;
pub mod step;
pub mod sync;

pub use builder::{build_desired_schema, build_document_type_tables};
pub use ddl::{
    build_add_column_sql, build_column_def, build_create_index_sql, build_create_table_sql,
    build_drop_column_sql, build_drop_index_sql, build_drop_table_sql, step_to_sql,
};
pub use diff::{DiffError, compute_diff};
pub use executor::{ExecutionSummary, execute_migration_plan};
pub use introspector::{SYSTEM_TABLES, introspect_database_schema};
pub use model::{
    ColumnDefinition, DatabaseSchema, ForeignKeyAction, ForeignKeyDefinition, IndexDefinition,
    SqlColumnType, TableDefinition, TableKind,
};
pub use planner::{MigrationPlan, PlannerError, plan_migrations};
pub use step::{MigrationStep, SafetyPolicy};
pub use sync::{
    SchemaSyncResult, generate_ddl_for_document_type, generate_ddl_for_registry,
    sync_dynamic_schemas, sync_dynamic_schemas_from_dir,
};
