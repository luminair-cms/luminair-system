//! Database persistence adapters, schema migrations, query generation, and health probes.

pub(crate) use crate as persistence;

pub mod connection;
pub mod error;
pub mod health;
pub mod migration;
pub mod naming;
pub mod query;
pub mod repositories;

pub use connection::{
    AWS_LAMBDA_ENV_VAR, AwsCredentials, AwsSdkDsqlTokenProvider, ConnectionTarget,
    DEFAULT_NON_SERVERLESS_MAX_CONNECTIONS, DSQL_ACQUIRE_TIMEOUT,
    DSQL_ALLOW_LOCAL_FALLBACK_ENV_VAR, DSQL_AUTH_TOKEN_ENV_VAR, DSQL_IDLE_TIMEOUT,
    DSQL_MAX_LIFETIME, DsqlPoolConfig, DsqlTokenGeneratorFn, DsqlTokenProvider,
    EnvironmentDsqlTokenProvider, SERVERLESS_DEFAULT_CONNECTIONS, SERVERLESS_MAX_CONNECTIONS,
    StaticDsqlTokenProvider, create_connection_pool, create_connection_pool_with_provider,
    detect_connection_target, generate_dsql_sigv4_token, is_lambda_environment,
    resolve_pool_max_connections,
};
pub use error::PersistenceError;
pub use health::{AlwaysHealthyChecker, HealthCheckService, HealthStatus, SqlxHealthChecker};
pub use migration::{
    MIGRATOR, PlannedMigrationStep, ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID,
    SafetyPolicy, SchemaSyncResult, preview_dynamic_schemas, run_static_migrations,
    sync_dynamic_schemas,
};
pub use naming::SchemaNaming;
pub use repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserProfileRepository, SqlxUserRoleAssignmentRepository,
};
pub use sqlx::{self, PgPool};
