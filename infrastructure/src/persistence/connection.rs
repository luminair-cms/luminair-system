//! Database connection factory supporting standard PostgreSQL and AWS Aurora DSQL endpoints.

use std::future::Future;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use url::Url;

use crate::persistence::error::PersistenceError;

// ---------------------------------------------------------------------------
// Types & Enums
// ---------------------------------------------------------------------------

/// Connection target classification parsed from a connection string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionTarget {
    /// Standard PostgreSQL instance (local, Docker, or CI).
    Postgres {
        /// Full connection string.
        url: String,
    },
    /// Amazon Aurora DSQL cluster endpoint requiring IAM authentication and TLS.
    AuroraDsql {
        /// DSQL cluster endpoint hostname (e.g. `cluster.dsql.us-east-1.on.aws`).
        cluster_endpoint: String,
        /// AWS region where the DSQL cluster is deployed.
        region: String,
        /// Database user name (defaults to `admin`).
        user: String,
        /// Database name (defaults to `postgres`).
        database: String,
    },
}

/// Dynamic authentication token provider for Amazon Aurora DSQL.
pub trait DsqlTokenProvider: Send + Sync {
    /// Generates a short-lived IAM authentication token for the given cluster and region.
    fn generate_auth_token(
        &self,
        cluster_endpoint: &str,
        region: &str,
    ) -> impl Future<Output = Result<String, PersistenceError>> + Send;
}

/// Static token provider returning a fixed string (primarily for testing and offline development).
#[derive(Debug, Clone)]
pub struct StaticDsqlTokenProvider {
    token: String,
}

/// Environment-driven token provider reading `LUMINAIR_DSQL_AUTH_TOKEN` with local test fallback.
#[derive(Debug, Default, Clone)]
pub struct EnvironmentDsqlTokenProvider;

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl StaticDsqlTokenProvider {
    /// Creates a new static token provider.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

impl DsqlTokenProvider for StaticDsqlTokenProvider {
    async fn generate_auth_token(
        &self,
        _cluster_endpoint: &str,
        _region: &str,
    ) -> Result<String, PersistenceError> {
        Ok(self.token.clone())
    }
}

impl DsqlTokenProvider for EnvironmentDsqlTokenProvider {
    async fn generate_auth_token(
        &self,
        _cluster_endpoint: &str,
        _region: &str,
    ) -> Result<String, PersistenceError> {
        if let Ok(token) = std::env::var("LUMINAIR_DSQL_AUTH_TOKEN") {
            return Ok(token);
        }
        Ok("mock-dsql-auth-token".to_string())
    }
}

// ---------------------------------------------------------------------------
// Public Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Creates a new PostgreSQL or Aurora DSQL connection pool using environment token resolution.
pub async fn create_connection_pool(
    database_url: &str,
    max_connections: u32,
) -> Result<PgPool, PersistenceError> {
    create_connection_pool_with_provider(
        database_url,
        max_connections,
        &EnvironmentDsqlTokenProvider,
    )
    .await
}

/// Creates a new connection pool using a custom [`DsqlTokenProvider`] for IAM credentials.
pub async fn create_connection_pool_with_provider<P>(
    database_url: &str,
    max_connections: u32,
    provider: &P,
) -> Result<PgPool, PersistenceError>
where
    P: DsqlTokenProvider + ?Sized,
{
    let target = detect_connection_target(database_url)?;
    match target {
        ConnectionTarget::Postgres { url } => build_postgres_pool(&url, max_connections).await,
        ConnectionTarget::AuroraDsql {
            cluster_endpoint,
            region,
            user,
            database,
        } => {
            build_aurora_dsql_pool(
                &cluster_endpoint,
                &region,
                &user,
                &database,
                max_connections,
                provider,
            )
            .await
        }
    }
}

/// Classifies a database connection URL into [`ConnectionTarget::Postgres`] or [`ConnectionTarget::AuroraDsql`].
pub fn detect_connection_target(url_str: &str) -> Result<ConnectionTarget, PersistenceError> {
    let parsed = Url::parse(url_str).map_err(|err| {
        PersistenceError::Configuration(format!("invalid database URL '{url_str}': {err}"))
    })?;

    let host = parsed.host_str().unwrap_or("localhost");
    let is_dsql = host.contains(".dsql.") && host.ends_with(".on.aws")
        || parsed
            .query_pairs()
            .any(|(k, v)| k == "dsql" && (v == "true" || v == "1"));

    if is_dsql {
        let region = extract_dsql_region(host, &parsed);
        let user = if parsed.username().is_empty() {
            "admin".to_string()
        } else {
            parsed.username().to_string()
        };
        let database = parsed
            .path()
            .trim_start_matches('/')
            .split('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("postgres")
            .to_string();

        Ok(ConnectionTarget::AuroraDsql {
            cluster_endpoint: host.to_string(),
            region,
            user,
            database,
        })
    } else {
        Ok(ConnectionTarget::Postgres {
            url: url_str.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Private Helpers (SLAP Level 1)
// ---------------------------------------------------------------------------

async fn build_postgres_pool(url: &str, max_connections: u32) -> Result<PgPool, PersistenceError> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await
        .map_err(PersistenceError::Sqlx)
}

async fn build_aurora_dsql_pool<P>(
    cluster_endpoint: &str,
    region: &str,
    user: &str,
    database: &str,
    max_connections: u32,
    provider: &P,
) -> Result<PgPool, PersistenceError>
where
    P: DsqlTokenProvider + ?Sized,
{
    let token = provider
        .generate_auth_token(cluster_endpoint, region)
        .await?;

    let connect_opts = PgConnectOptions::new()
        .host(cluster_endpoint)
        .port(5432)
        .username(user)
        .password(&token)
        .database(database)
        .ssl_mode(PgSslMode::Require);

    PgPoolOptions::new()
        .max_connections(max_connections)
        .max_lifetime(Some(Duration::from_secs(10 * 60)))
        .idle_timeout(Some(Duration::from_secs(5 * 60)))
        .acquire_timeout(Duration::from_secs(30))
        .connect_with(connect_opts)
        .await
        .map_err(PersistenceError::Sqlx)
}

fn extract_dsql_region(host: &str, url: &Url) -> String {
    if let Some((_, region_val)) = url.query_pairs().find(|(k, _)| k == "region") {
        return region_val.to_string();
    }

    let parts: Vec<&str> = host.split('.').collect();
    if let Some(pos) = parts.iter().position(|p| *p == "dsql")
        && let Some(reg) = parts.get(pos + 1)
        && *reg != "on"
        && *reg != "aws"
    {
        return (*reg).to_string();
    }

    "us-east-1".to_string()
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_postgres_connection_target() {
        let url = "postgres://luminair:secret@localhost:5432/luminair";
        let target = detect_connection_target(url).expect("valid postgres URL");
        assert_eq!(
            target,
            ConnectionTarget::Postgres {
                url: url.to_string()
            }
        );
    }

    #[test]
    fn test_detect_aurora_dsql_connection_target_from_hostname() {
        let url = "postgres://admin@a1b2c3d4.dsql.eu-west-1.on.aws:5432/postgres";
        let target = detect_connection_target(url).expect("valid dsql URL");
        assert_eq!(
            target,
            ConnectionTarget::AuroraDsql {
                cluster_endpoint: "a1b2c3d4.dsql.eu-west-1.on.aws".to_string(),
                region: "eu-west-1".to_string(),
                user: "admin".to_string(),
                database: "postgres".to_string(),
            }
        );
    }

    #[test]
    fn test_detect_aurora_dsql_connection_target_with_query_param() {
        let url = "postgres://custom_user:pass@mydb.internal/app_db?dsql=true&region=ap-northeast-1";
        let target = detect_connection_target(url).expect("valid dsql URL");
        assert_eq!(
            target,
            ConnectionTarget::AuroraDsql {
                cluster_endpoint: "mydb.internal".to_string(),
                region: "ap-northeast-1".to_string(),
                user: "custom_user".to_string(),
                database: "app_db".to_string(),
            }
        );
    }

    #[test]
    fn test_detect_invalid_url_returns_configuration_error() {
        let err = detect_connection_target("not-a-valid-url").expect_err("should fail parsing");
        assert!(matches!(err, PersistenceError::Configuration(_)));
    }

    #[tokio::test]
    async fn test_static_dsql_token_provider() {
        let provider = StaticDsqlTokenProvider::new("test-token-12345");
        let token = provider
            .generate_auth_token("cluster.dsql.us-east-1.on.aws", "us-east-1")
            .await
            .expect("token generation");
        assert_eq!(token, "test-token-12345");
    }
}
