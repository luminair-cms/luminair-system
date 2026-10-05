pub mod persistence;

pub use persistence::migration::{
    MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID, run_static_migrations,
};
