//! Database persistence adapters, schema migrations, and query generation.

pub mod connection;
pub mod error;
pub mod migration;
pub mod naming;
pub mod query;
pub mod repositories;

pub use connection::{
    ConnectionTarget, DsqlTokenProvider, EnvironmentDsqlTokenProvider, StaticDsqlTokenProvider,
    create_connection_pool, create_connection_pool_with_provider, detect_connection_target,
};
pub use error::PersistenceError;
pub use migration::run_static_migrations;
pub use repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};
