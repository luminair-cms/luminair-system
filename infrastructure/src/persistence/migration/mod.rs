pub mod dynamic;
pub mod embedded;

pub use dynamic::{
    generate_ddl_for_document_type, generate_ddl_for_registry,
    sync_dynamic_schemas as sync_schemas,
    sync_dynamic_schemas_from_dir as sync_schemas_from_dir,
    SafetyPolicy, SchemaSyncResult as SyncResult,
};
pub use embedded::{
    MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID,
    run_migrations as run_static_migrations,
};
