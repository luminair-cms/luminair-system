pub mod api;
pub mod auth;
pub mod cli;
pub mod container;
pub mod health;
pub mod persistence;
pub mod schema_loader;

pub use container::{AppContainer, PersistenceContainer};
pub use persistence::migration::{
    MIGRATOR, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID, run_static_migrations,
};
