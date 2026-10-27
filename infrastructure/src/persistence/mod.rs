//! Database persistence adapters, schema migrations, and query generation.

pub mod error;
pub mod migration;
pub mod naming;
pub mod query;
pub mod repositories;

pub use error::PersistenceError;
pub use migration::run_static_migrations;
pub use repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};
