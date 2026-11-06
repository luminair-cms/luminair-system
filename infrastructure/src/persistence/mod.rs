//! Database persistence adapters, schema migrations, and query generation.

pub(crate) mod connection;
pub(crate) mod error;
pub mod migration;
pub mod naming;
pub mod query;
pub mod repositories;

pub(crate) use connection::create_connection_pool;
pub(crate) use error::PersistenceError;
pub use repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserProfileRepository, SqlxUserRoleAssignmentRepository,
};
