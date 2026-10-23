pub mod dynamic_sync;
pub mod embedded;

pub use dynamic_sync::{
    build_add_column_ddl, build_create_table_ddl, build_junction_tables_ddl,
    build_table_indexes_ddl, generate_ddl_for_document_type, generate_ddl_for_registry,
    sync_schemas, sync_schemas_from_dir, SafetyPolicy, SyncResult,
};
pub use embedded::{
    MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID,
    run_migrations as run_static_migrations,
};
