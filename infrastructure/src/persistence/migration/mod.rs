pub mod embedded;

pub use embedded::{
    MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID,
    run_migrations as run_static_migrations,
};
