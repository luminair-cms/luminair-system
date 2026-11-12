//! Database connection factory supporting standard PostgreSQL and AWS Aurora DSQL endpoints.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use url::Url;

use crate::persistence::error::PersistenceError;

// ---------------------------------------------------------------------------
// Constants & Configuration
// ---------------------------------------------------------------------------

/// Default max connections for standard (non-serverless) environments when 0 is requested.
pub const DEFAULT_NON_SERVERLESS_MAX_CONNECTIONS: u32 = 20;

/// Default connection pool size for AWS Lambda serverless execution.
pub const SERVERLESS_DEFAULT_CONNECTIONS: u32 = 1;

/// Maximum allowable connections for AWS Lambda execution sandboxes.
pub const SERVERLESS_MAX_CONNECTIONS: u32 = 2;

/// Maximum connection lifetime for Aurora DSQL connection pools (10 minutes, inside 15m IAM window).
pub const DSQL_MAX_LIFETIME: Duration = Duration::from_secs(10 * 60);

/// Idle connection timeout for Aurora DSQL connection pools (5 minutes).
pub const DSQL_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Acquisition timeout for Aurora DSQL connection pools (30 seconds).
pub const DSQL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(30);

/// Environment variable indicating execution within an AWS Lambda runtime sandbox.
pub const AWS_LAMBDA_ENV_VAR: &str = "AWS_LAMBDA_FUNCTION_NAME";

/// Environment variable for an explicit static or test Aurora DSQL authentication token.
pub const DSQL_AUTH_TOKEN_ENV_VAR: &str = "LUMINAIR_DSQL_AUTH_TOKEN";

/// Environment variable to explicitly allow local/mock fallback tokens for Aurora DSQL.
pub const DSQL_ALLOW_LOCAL_FALLBACK_ENV_VAR: &str = "LUMINAIR_DSQL_ALLOW_LOCAL_FALLBACK";

// ---------------------------------------------------------------------------
// Types & Enums
// ---------------------------------------------------------------------------

/// Type alias for dynamic token generator closures.
pub type DsqlTokenGeneratorFn =
    Arc<dyn Fn(&str, &str) -> Result<String, PersistenceError> + Send + Sync>;

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

/// Connection pool configuration parameters for Aurora DSQL and PostgreSQL pools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DsqlPoolConfig {
    /// Maximum number of active connections in the pool.
    pub max_connections: u32,
    /// Maximum connection lifetime before recycling.
    pub max_lifetime: Option<Duration>,
    /// Duration after which an idle connection is closed.
    pub idle_timeout: Option<Duration>,
    /// Maximum time to wait when acquiring a connection from the pool.
    pub acquire_timeout: Duration,
}

impl DsqlPoolConfig {
    /// Creates a pool configuration tailored for Aurora DSQL.
    #[must_use]
    pub fn for_aurora_dsql(requested_max_connections: u32, is_lambda: bool) -> Self {
        let max_connections = resolve_pool_max_connections(requested_max_connections, is_lambda);
        Self {
            max_connections,
            max_lifetime: Some(DSQL_MAX_LIFETIME),
            idle_timeout: Some(DSQL_IDLE_TIMEOUT),
            acquire_timeout: DSQL_ACQUIRE_TIMEOUT,
        }
    }

    /// Creates a pool configuration for standard PostgreSQL.
    #[must_use]
    pub fn for_postgres(requested_max_connections: u32, is_lambda: bool) -> Self {
        let max_connections = resolve_pool_max_connections(requested_max_connections, is_lambda);
        Self {
            max_connections,
            max_lifetime: None,
            idle_timeout: None,
            acquire_timeout: DSQL_ACQUIRE_TIMEOUT,
        }
    }

    /// Converts this configuration into SQLx [`PgPoolOptions`].
    #[must_use]
    pub fn to_pool_options(&self) -> PgPoolOptions {
        let mut opts = PgPoolOptions::new()
            .max_connections(self.max_connections)
            .acquire_timeout(self.acquire_timeout);
        if let Some(lifetime) = self.max_lifetime {
            opts = opts.max_lifetime(lifetime);
        }
        if let Some(idle) = self.idle_timeout {
            opts = opts.idle_timeout(idle);
        }
        opts
    }
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

/// Environment-driven token provider reading `LUMINAIR_DSQL_AUTH_TOKEN` with local test fallback.
#[derive(Debug, Default, Clone)]
pub struct EnvironmentDsqlTokenProvider;

impl DsqlTokenProvider for EnvironmentDsqlTokenProvider {
    async fn generate_auth_token(
        &self,
        _cluster_endpoint: &str,
        _region: &str,
    ) -> Result<String, PersistenceError> {
        if let Ok(token) = std::env::var(DSQL_AUTH_TOKEN_ENV_VAR) {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        Ok("mock-dsql-auth-token".to_string())
    }
}

/// AWS credentials used for SigV4 signing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsCredentials {
    /// AWS Access Key ID.
    pub access_key_id: String,
    /// AWS Secret Access Key.
    pub secret_access_key: String,
    /// Optional AWS Session Token (for STS / assumed roles).
    pub session_token: Option<String>,
}

impl AwsCredentials {
    /// Creates explicit AWS credentials.
    #[must_use]
    pub fn new(
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        session_token: Option<String>,
    ) -> Self {
        Self {
            access_key_id: access_key_id.into(),
            secret_access_key: secret_access_key.into(),
            session_token,
        }
    }

    /// Attempts to resolve AWS credentials from standard environment variables:
    /// `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and optional `AWS_SESSION_TOKEN`.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let access_key_id = std::env::var("AWS_ACCESS_KEY_ID").ok()?;
        let secret_access_key = std::env::var("AWS_SECRET_ACCESS_KEY").ok()?;
        if access_key_id.trim().is_empty() || secret_access_key.trim().is_empty() {
            return None;
        }
        let session_token = std::env::var("AWS_SESSION_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        Some(Self {
            access_key_id: access_key_id.trim().to_string(),
            secret_access_key: secret_access_key.trim().to_string(),
            session_token,
        })
    }
}

/// Configurable AWS SDK-compatible IAM token provider for Amazon Aurora DSQL.
///
/// Resolves authentication tokens using:
/// 1. Custom token generator closure (if configured).
/// 2. Explicit static token override (e.g. for testing).
/// 3. Environment variable `LUMINAIR_DSQL_AUTH_TOKEN`.
/// 4. AWS SigV4 presigned token generated from explicit or environment credentials
///    (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and optional `AWS_SESSION_TOKEN`).
/// 5. Local fallback token (`mock-dsql-auth-token`) if local fallback is allowed.
///
/// Returns a descriptive [`PersistenceError::Configuration`] if no valid credentials or fallback are available.
#[derive(Clone)]
pub struct AwsSdkDsqlTokenProvider {
    credentials: Option<AwsCredentials>,
    static_token: Option<String>,
    action: String,
    allow_local_fallback: bool,
    token_generator: Option<DsqlTokenGeneratorFn>,
}

impl Default for AwsSdkDsqlTokenProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AwsSdkDsqlTokenProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsSdkDsqlTokenProvider")
            .field("has_credentials", &self.credentials.is_some())
            .field("has_static_token", &self.static_token.is_some())
            .field("action", &self.action)
            .field("allow_local_fallback", &self.allow_local_fallback)
            .field("has_custom_generator", &self.token_generator.is_some())
            .finish()
    }
}

impl AwsSdkDsqlTokenProvider {
    /// Creates a new AWS SDK token provider reading configuration from the environment.
    #[must_use]
    pub fn new() -> Self {
        let allow_local_fallback = match std::env::var(DSQL_ALLOW_LOCAL_FALLBACK_ENV_VAR) {
            Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
            Err(_) => false,
        };

        Self {
            credentials: AwsCredentials::from_env(),
            static_token: None,
            action: "DbConnectAdmin".to_string(),
            allow_local_fallback,
            token_generator: None,
        }
    }

    /// Sets explicit AWS credentials for SigV4 token generation.
    #[must_use]
    pub fn with_credentials(mut self, credentials: AwsCredentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// Sets an explicit static token override (useful for testing).
    #[must_use]
    pub fn with_static_token(mut self, token: impl Into<String>) -> Self {
        self.static_token = Some(token.into());
        self
    }

    /// Sets the DSQL action (e.g. `DbConnectAdmin` or `DbConnect`).
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }

    /// Configures whether local/mock fallback tokens are permitted when credentials are missing.
    #[must_use]
    pub fn with_local_fallback(mut self, allow: bool) -> Self {
        self.allow_local_fallback = allow;
        self
    }

    /// Configures a custom token generation closure.
    #[must_use]
    pub fn with_generator<F>(mut self, generator: F) -> Self
    where
        F: Fn(&str, &str) -> Result<String, PersistenceError> + Send + Sync + 'static,
    {
        self.token_generator = Some(Arc::new(generator));
        self
    }
}

impl DsqlTokenProvider for AwsSdkDsqlTokenProvider {
    async fn generate_auth_token(
        &self,
        cluster_endpoint: &str,
        region: &str,
    ) -> Result<String, PersistenceError> {
        // 1. Custom generator takes highest precedence
        if let Some(generator) = &self.token_generator {
            return generator(cluster_endpoint, region);
        }

        // 2. Static token override
        if let Some(token) = &self.static_token {
            return Ok(token.clone());
        }

        // 3. Environment variable override (LUMINAIR_DSQL_AUTH_TOKEN)
        if let Ok(token) = std::env::var(DSQL_AUTH_TOKEN_ENV_VAR) {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }

        // 4. AWS SigV4 generation via credentials (explicit or environment)
        let credentials = self.credentials.clone().or_else(AwsCredentials::from_env);
        if let Some(creds) = credentials {
            return generate_dsql_sigv4_token(
                cluster_endpoint,
                region,
                &self.action,
                &creds,
                chrono::Utc::now(),
            );
        }

        // 5. Local/mock fallback if allowed
        if self.allow_local_fallback {
            tracing::warn!(
                cluster_endpoint,
                region,
                "missing AWS credentials for Aurora DSQL; using mock fallback token"
            );
            return Ok("mock-dsql-auth-token".to_string());
        }

        // 6. Descriptive configuration error
        Err(PersistenceError::Configuration(format!(
            "failed to generate Aurora DSQL IAM token for cluster '{cluster_endpoint}' (region: '{region}'): \
             missing AWS credentials (set AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY, \
             provide LUMINAIR_DSQL_AUTH_TOKEN, or enable local fallback via LUMINAIR_DSQL_ALLOW_LOCAL_FALLBACK=true)"
        )))
    }
}

// ---------------------------------------------------------------------------
// Public Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Determines whether the current process is executing within an AWS Lambda runtime.
#[must_use]
pub fn is_lambda_environment() -> bool {
    std::env::var(AWS_LAMBDA_ENV_VAR).is_ok()
}

/// Computes the effective maximum pool connections applying serverless safeguards.
///
/// In an AWS Lambda environment (`AWS_LAMBDA_FUNCTION_NAME` is set):
/// - A requested value of 0 defaults to 1.
/// - The pool size is clamped between 1 and 2 (`clamp(1, 2)`).
/// - If the requested pool size exceeded 2, a warning message is logged:
///   `"clamping connection pool to serverless maximum (2) for AWS Lambda"`.
///
/// In a non-Lambda environment:
/// - A requested value of 0 defaults to 20 (`DEFAULT_NON_SERVERLESS_MAX_CONNECTIONS`).
/// - Otherwise, the requested pool size is preserved.
#[must_use]
pub fn resolve_pool_max_connections(requested_max_connections: u32, is_lambda: bool) -> u32 {
    if is_lambda {
        let normalized = if requested_max_connections == 0 {
            1
        } else {
            requested_max_connections
        };

        if requested_max_connections > 2 {
            tracing::warn!(
                requested = requested_max_connections,
                effective = 2,
                "clamping connection pool to serverless maximum (2) for AWS Lambda"
            );
        } else {
            tracing::info!(
                requested = requested_max_connections,
                effective = normalized.clamp(1, 2),
                "configured serverless connection pool for AWS Lambda"
            );
        }
        normalized.clamp(1, 2)
    } else if requested_max_connections == 0 {
        DEFAULT_NON_SERVERLESS_MAX_CONNECTIONS
    } else {
        requested_max_connections
    }
}

/// Creates a new PostgreSQL or Aurora DSQL connection pool with serverless safeguards.
///
/// In an AWS Lambda environment, the connection count is clamped to `[1, 2]`.
/// In standard environments, larger pool sizes (e.g. 20, 50) are preserved.
pub async fn create_connection_pool(
    database_url: &str,
    requested_max_connections: u32,
) -> Result<PgPool, PersistenceError> {
    let effective_max =
        resolve_pool_max_connections(requested_max_connections, is_lambda_environment());
    create_connection_pool_with_provider(
        database_url,
        effective_max,
        &AwsSdkDsqlTokenProvider::default(),
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
        .acquire_timeout(DSQL_ACQUIRE_TIMEOUT)
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

    let config = DsqlPoolConfig {
        max_connections,
        max_lifetime: Some(DSQL_MAX_LIFETIME),
        idle_timeout: Some(DSQL_IDLE_TIMEOUT),
        acquire_timeout: DSQL_ACQUIRE_TIMEOUT,
    };

    config
        .to_pool_options()
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

type HmacSha256 = Hmac<Sha256>;

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>, PersistenceError> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|e| {
        PersistenceError::Configuration(format!("HMAC key initialization error: {e}"))
    })?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn uri_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{b:02X}"));
            }
        }
    }
    encoded
}

/// Generates an AWS SigV4 presigned authentication token for Amazon Aurora DSQL.
///
/// Aurora DSQL authentication uses a presigned URL query string for the `DbConnect` / `DbConnectAdmin` action
/// against the cluster endpoint, valid for 15 minutes (900 seconds).
pub fn generate_dsql_sigv4_token(
    cluster_endpoint: &str,
    region: &str,
    action: &str,
    credentials: &AwsCredentials,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Result<String, PersistenceError> {
    if cluster_endpoint.trim().is_empty() {
        return Err(PersistenceError::Configuration(
            "cluster endpoint cannot be empty for Aurora DSQL token generation".to_string(),
        ));
    }
    if region.trim().is_empty() {
        return Err(PersistenceError::Configuration(
            "AWS region cannot be empty for Aurora DSQL token generation".to_string(),
        ));
    }

    let date_stamp = timestamp.format("%Y%m%d").to_string();
    let amz_date = timestamp.format("%Y%m%dT%H%M%SZ").to_string();
    let credential_scope = format!("{date_stamp}/{region}/dsql/aws4_request");
    let expires = "900";

    // Alphabetically sorted query parameters:
    // 1. Action
    // 2. X-Amz-Algorithm
    // 3. X-Amz-Credential
    // 4. X-Amz-Date
    // 5. X-Amz-Expires
    // (optional) 6. X-Amz-Security-Token
    // 7. X-Amz-SignedHeaders
    let encoded_action = format!("Action={}", uri_encode(action));
    let encoded_algorithm = "X-Amz-Algorithm=AWS4-HMAC-SHA256".to_string();
    let encoded_credential = format!(
        "X-Amz-Credential={}%2F{}",
        uri_encode(&credentials.access_key_id),
        uri_encode(&credential_scope)
    );
    let encoded_date = format!("X-Amz-Date={}", uri_encode(&amz_date));
    let encoded_expires = format!("X-Amz-Expires={}", uri_encode(expires));
    let encoded_signed_headers = "X-Amz-SignedHeaders=host".to_string();

    let mut query_params = vec![
        encoded_action,
        encoded_algorithm,
        encoded_credential,
        encoded_date,
        encoded_expires,
    ];

    if let Some(token) = &credentials.session_token {
        query_params.push(format!("X-Amz-Security-Token={}", uri_encode(token)));
    }
    query_params.push(encoded_signed_headers);

    // Ensure parameters are sorted in ascending byte order
    query_params.sort();
    let canonical_query = query_params.join("&");

    let canonical_headers = format!("host:{cluster_endpoint}\n");
    let signed_headers = "host";
    let payload_hash = hex::encode(Sha256::digest(b""));

    let canonical_request =
        format!("GET\n/\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}");

    let hashed_canonical_request = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let string_to_sign =
        format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{hashed_canonical_request}");

    let k_secret = format!("AWS4{}", credentials.secret_access_key);
    let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, b"dsql")?;
    let k_signing = hmac_sha256(&k_service, b"aws4_request")?;

    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes())?);
    let token = format!("{canonical_query}&X-Amz-Signature={signature}");

    Ok(token)
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
        let url =
            "postgres://custom_user:pass@mydb.internal/app_db?dsql=true&region=ap-northeast-1";
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

    #[test]
    fn test_serverless_lambda_pool_sizing_clamps_to_range() {
        // Lambda defaults 0 to 1
        assert_eq!(resolve_pool_max_connections(0, true), 1);
        // 1 remains 1
        assert_eq!(resolve_pool_max_connections(1, true), 1);
        // 2 remains 2
        assert_eq!(resolve_pool_max_connections(2, true), 2);
        // Larger sizes clamped to maximum 2
        assert_eq!(resolve_pool_max_connections(3, true), 2);
        assert_eq!(resolve_pool_max_connections(20, true), 2);
        assert_eq!(resolve_pool_max_connections(50, true), 2);
    }

    #[test]
    fn test_non_lambda_pool_sizing_preserves_values() {
        // Non-lambda defaults 0 to DEFAULT_NON_SERVERLESS_MAX_CONNECTIONS (20)
        assert_eq!(
            resolve_pool_max_connections(0, false),
            DEFAULT_NON_SERVERLESS_MAX_CONNECTIONS
        );
        // Explicit values preserved
        assert_eq!(resolve_pool_max_connections(1, false), 1);
        assert_eq!(resolve_pool_max_connections(10, false), 10);
        assert_eq!(resolve_pool_max_connections(20, false), 20);
        assert_eq!(resolve_pool_max_connections(50, false), 50);
        assert_eq!(resolve_pool_max_connections(100, false), 100);
    }

    #[test]
    fn test_dsql_pool_config_timeouts_and_lifetime() {
        let dsql_config = DsqlPoolConfig::for_aurora_dsql(50, false);
        assert_eq!(dsql_config.max_connections, 50);
        assert_eq!(dsql_config.max_lifetime, Some(Duration::from_secs(10 * 60)));
        assert_eq!(dsql_config.idle_timeout, Some(Duration::from_secs(5 * 60)));
        assert_eq!(dsql_config.acquire_timeout, Duration::from_secs(30));

        let lambda_config = DsqlPoolConfig::for_aurora_dsql(50, true);
        assert_eq!(lambda_config.max_connections, 2); // clamped
        assert_eq!(
            lambda_config.max_lifetime,
            Some(Duration::from_secs(10 * 60))
        );
        assert_eq!(
            lambda_config.idle_timeout,
            Some(Duration::from_secs(5 * 60))
        );
        assert_eq!(lambda_config.acquire_timeout, Duration::from_secs(30));

        let pg_config = DsqlPoolConfig::for_postgres(50, false);
        assert_eq!(pg_config.max_connections, 50);
        assert_eq!(pg_config.max_lifetime, None);
        assert_eq!(pg_config.idle_timeout, None);
        assert_eq!(pg_config.acquire_timeout, Duration::from_secs(30));

        // Ensure to_pool_options builds cleanly without panic
        let _opts = dsql_config.to_pool_options();
        let _lambda_opts = lambda_config.to_pool_options();
        let _pg_opts = pg_config.to_pool_options();
    }

    #[tokio::test]
    async fn test_aws_sdk_dsql_token_provider_with_static_override() {
        let provider = AwsSdkDsqlTokenProvider::new().with_static_token("explicit-test-token");
        let token = provider
            .generate_auth_token("my-cluster.dsql.us-east-1.on.aws", "us-east-1")
            .await
            .expect("static token should resolve");
        assert_eq!(token, "explicit-test-token");
    }

    #[tokio::test]
    async fn test_aws_sdk_dsql_token_provider_with_generator() {
        let provider = AwsSdkDsqlTokenProvider::new()
            .with_generator(|endpoint, region| Ok(format!("generated-for-{endpoint}-{region}")));
        let token = provider
            .generate_auth_token("test-endpoint", "ap-southeast-1")
            .await
            .expect("custom generator should resolve");
        assert_eq!(token, "generated-for-test-endpoint-ap-southeast-1");
    }

    #[tokio::test]
    async fn test_aws_sdk_dsql_token_provider_missing_credentials_fails_with_configuration_error() {
        let provider = AwsSdkDsqlTokenProvider {
            credentials: None,
            static_token: None,
            action: "DbConnectAdmin".to_string(),
            allow_local_fallback: false,
            token_generator: None,
        };

        let err = provider
            .generate_auth_token("dummy-cluster", "us-east-1")
            .await
            .expect_err("should fail when credentials are not available");

        match err {
            PersistenceError::Configuration(msg) => {
                assert!(msg.contains("missing AWS credentials"));
                assert!(msg.contains("dummy-cluster"));
                assert!(msg.contains("us-east-1"));
            }
            other => panic!("expected Configuration error, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_aws_sdk_dsql_token_provider_local_fallback() {
        let provider = AwsSdkDsqlTokenProvider {
            credentials: None,
            static_token: None,
            action: "DbConnectAdmin".to_string(),
            allow_local_fallback: true,
            token_generator: None,
        };

        let token = provider
            .generate_auth_token("local-cluster", "us-east-1")
            .await
            .expect("fallback token should resolve");
        assert_eq!(token, "mock-dsql-auth-token");
    }

    #[tokio::test]
    async fn test_aws_sdk_dsql_token_provider_sigv4_generation() {
        let creds = AwsCredentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            None,
        );
        let provider = AwsSdkDsqlTokenProvider::new()
            .with_credentials(creds)
            .with_action("DbConnectAdmin");

        let token = provider
            .generate_auth_token("a1b2c3d4.dsql.us-east-1.on.aws", "us-east-1")
            .await
            .expect("sigv4 token generation should succeed");

        assert!(token.contains("Action=DbConnectAdmin"));
        assert!(token.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(token.contains("X-Amz-Credential=AKIAIOSFODNN7EXAMPLE"));
        assert!(token.contains("X-Amz-Expires=900"));
        assert!(token.contains("X-Amz-SignedHeaders=host"));
        assert!(token.contains("X-Amz-Signature="));
    }

    #[tokio::test]
    async fn test_aws_sdk_dsql_token_provider_sigv4_with_session_token() {
        let creds = AwsCredentials::new(
            "ASIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            Some("AQoDYXdzEJr1111111111111111111111111".to_string()),
        );
        let provider = AwsSdkDsqlTokenProvider::new()
            .with_credentials(creds)
            .with_action("DbConnect");

        let token = provider
            .generate_auth_token("a1b2c3d4.dsql.eu-central-1.on.aws", "eu-central-1")
            .await
            .expect("sigv4 token with session token should succeed");

        assert!(token.contains("Action=DbConnect"));
        assert!(token.contains("X-Amz-Security-Token="));
        assert!(token.contains("eu-central-1"));
    }

    #[test]
    fn test_generate_dsql_sigv4_token_validation_errors() {
        let creds = AwsCredentials::new("key", "secret", None);
        let now = chrono::Utc::now();

        let err1 = generate_dsql_sigv4_token("", "us-east-1", "DbConnect", &creds, now)
            .expect_err("empty endpoint");
        assert!(matches!(err1, PersistenceError::Configuration(_)));

        let err2 = generate_dsql_sigv4_token("my-cluster", "", "DbConnect", &creds, now)
            .expect_err("empty region");
        assert!(matches!(err2, PersistenceError::Configuration(_)));
    }

    #[tokio::test]
    async fn test_environment_dsql_token_provider_fallback() {
        let provider = EnvironmentDsqlTokenProvider;
        let token = provider
            .generate_auth_token("cluster", "region")
            .await
            .expect("environment provider fallback");
        assert!(!token.is_empty());
    }
}
