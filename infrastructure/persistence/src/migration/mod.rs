mod dynamic;
mod embedded;

pub use dynamic::{
    PlannedMigrationStep, SafetyPolicy, SchemaSyncResult, preview_dynamic_schemas,
    sync_dynamic_schemas,
};
pub use embedded::{
    MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID, run_static_migrations,
};

#[cfg(test)]
pub(crate) use dynamic::SqlColumnType;
