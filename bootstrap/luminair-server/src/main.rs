//! Luminair HTTP microservice daemon entry point (`luminair-server`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use domain::auth::role::RoleId;
use domain::auth::user::ServiceId;
use luminair_auth::{OidcTokenValidator, ServiceTokenRegistry, SqlxAuthResolver};
use luminair_http::AppContainer;
use luminair_http::routes::build_router;
use luminair_persistence::create_connection_pool;
use luminair_persistence::migration::{
    ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID, SafetyPolicy,
    run_static_migrations, sync_dynamic_schemas,
};
use luminair_schema_loader::load_system_context_from_dir;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceTokenConfig {
    token: String,
    service_id: String,
    roles: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_tracing();

    let host = std::env::var("LUMINAIR_HTTP_HOST")
        .or_else(|_| std::env::var("LUMINAIR_HOST"))
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("LUMINAIR_HTTP_PORT")
        .or_else(|_| std::env::var("LUMINAIR_PORT"))
        .or_else(|_| std::env::var("PORT"))
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .map_err(|e| format!("invalid LUMINAIR_HTTP_PORT: {e}"))?;

    let schema_dir: PathBuf = std::env::var("LUMINAIR_SCHEMA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./config/schemas"));

    let db_url = std::env::var("LUMINAIR_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| "LUMINAIR_DATABASE_URL (or DATABASE_URL) is required".to_string())?;

    let max_connections: u32 = std::env::var("LUMINAIR_MAX_DB_CONNECTIONS")
        .unwrap_or_else(|_| "20".to_string())
        .parse()
        .unwrap_or(20)
        .clamp(20, 50);

    let auto_migrate = std::env::var("LUMINAIR_AUTO_MIGRATE")
        .map(|v| parse_bool(&v))
        .unwrap_or(false);

    let safety_policy = std::env::var("LUMINAIR_SAFETY_POLICY")
        .map(|v| parse_safety_policy(&v))
        .unwrap_or(SafetyPolicy::AdditiveOnly);

    tracing::info!(
        schema_dir = %schema_dir.display(),
        "Loading schema definitions into SystemContext..."
    );
    let context = load_system_context_from_dir(Path::new(&schema_dir))
        .map_err(|e| format!("failed to load schemas: {e}"))?;

    tracing::info!(max_connections, "Connecting to database...");
    let pool = create_connection_pool(&db_url, max_connections)
        .await
        .map_err(|e| format!("database connection failed: {e}"))?;

    if auto_migrate {
        tracing::info!("Running auto-migrations prior to starting HTTP listener...");
        run_static_migrations(&pool)
            .await
            .map_err(|e| format!("static migrations failed: {e}"))?;
        sync_dynamic_schemas(&pool, &context.schema, safety_policy)
            .await
            .map_err(|e| format!("dynamic schema sync failed: {e}"))?;
        tracing::info!("Auto-migrations successfully applied");
    }

    let oidc_validator = build_oidc_validator().await?;
    let service_tokens = Arc::new(build_service_token_registry()?);

    let auth_resolver = Arc::new(SqlxAuthResolver::from_pool(
        pool.clone(),
        oidc_validator,
        service_tokens,
    ));

    let container = AppContainer::new(pool, context, auth_resolver);
    let router = build_router(container.http_state());

    let bind_addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| format!("failed to bind TCP listener on {bind_addr}: {e}"))?;

    let local_addr = listener
        .local_addr()
        .map_err(|e| format!("failed to get local address: {e}"))?;

    tracing::info!(
        host = %host,
        port = %local_addr.port(),
        doc_types = %context.schema.iterate().count(),
        "Starting Luminair HTTP Service (luminair-server)"
    );

    luminair_http::axum::serve(listener, router)
        .with_graceful_shutdown(wait_for_shutdown_signal())
        .await
        .map_err(|e| format!("server error: {e}"))?;

    tracing::info!("Luminair HTTP Service shutdown cleanly");
    Ok(())
}

async fn build_oidc_validator()
-> Result<Arc<OidcTokenValidator>, Box<dyn std::error::Error + Send + Sync>> {
    let issuer = std::env::var("LUMINAIR_OIDC_ISSUER").ok();
    let audience = std::env::var("LUMINAIR_OIDC_AUDIENCE").ok();

    if let Ok(jwks_url) = std::env::var("LUMINAIR_OIDC_JWKS_URL") {
        let jwks_json = fetch_jwks_with_retry(&jwks_url).await?;
        let validator = OidcTokenValidator::from_jwks_json(&jwks_json, issuer, audience)
            .map_err(|e| format!("invalid OIDC JWKS from '{jwks_url}': {e}"))?;
        return Ok(Arc::new(validator));
    }

    if let Ok(secret) = std::env::var("LUMINAIR_OIDC_JWT_SECRET") {
        return Ok(Arc::new(OidcTokenValidator::new_hs256(
            secret.as_bytes(),
            issuer,
            audience,
        )));
    }

    if let Ok(pem) = std::env::var("LUMINAIR_OIDC_JWT_PUBLIC_KEY_PEM") {
        let validator = OidcTokenValidator::from_rsa_pem(pem.as_bytes(), issuer, audience)
            .map_err(|e| format!("invalid OIDC RS256 public key PEM: {e}"))?;
        return Ok(Arc::new(validator));
    }

    tracing::warn!("No OIDC credentials configured; running with authentication disabled");
    Ok(Arc::new(OidcTokenValidator::disabled()))
}

async fn fetch_jwks_with_retry(
    url: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    const MAX_ATTEMPTS: u32 = 10;
    const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let mut last_error = String::new();
    for attempt in 1..=MAX_ATTEMPTS {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await?;
                return Ok(body);
            }
            Ok(resp) => {
                last_error = format!("HTTP {}", resp.status());
            }
            Err(e) => {
                last_error = e.to_string();
            }
        }

        if attempt < MAX_ATTEMPTS {
            tracing::warn!(
                attempt,
                max_attempts = MAX_ATTEMPTS,
                url,
                error = %last_error,
                "Failed to fetch OIDC JWKS; retrying in 500ms..."
            );
            tokio::time::sleep(RETRY_DELAY).await;
        }
    }

    Err(format!(
        "failed to fetch OIDC JWKS from '{url}' after {MAX_ATTEMPTS} attempts: {last_error}"
    )
    .into())
}

fn build_service_token_registry()
-> Result<ServiceTokenRegistry, Box<dyn std::error::Error + Send + Sync>> {
    let mut registry = ServiceTokenRegistry::new();
    let raw = match std::env::var("LUMINAIR_SERVICE_TOKENS") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(registry),
    };

    let entries: Vec<ServiceTokenConfig> = if raw.trim().starts_with('[') {
        serde_json::from_str(&raw)
            .map_err(|e| format!("invalid JSON in LUMINAIR_SERVICE_TOKENS: {e}"))?
    } else {
        raw.split(';')
            .filter(|item| !item.trim().is_empty())
            .map(|item| {
                let parts: Vec<&str> = item.split(':').collect();
                if parts.len() != 3 {
                    return Err(format!(
                        "invalid service token entry '{item}', expected 'token:service_id:role1,role2'"
                    ));
                }
                let token = parts[0].trim().to_string();
                let service_id = parts[1].trim().to_string();
                let roles = parts[2]
                    .split(',')
                    .map(|r| r.trim().to_string())
                    .filter(|r| !r.is_empty())
                    .collect();
                Ok(ServiceTokenConfig {
                    token,
                    service_id,
                    roles,
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    for entry in entries {
        let service_id = ServiceId::try_new(entry.service_id)
            .map_err(|e| format!("invalid service_id in token config: {e}"))?;
        let mut role_ids = Vec::with_capacity(entry.roles.len());
        for role_str in entry.roles {
            let role_id = match role_str.as_str() {
                "admin" => RoleId::new(ROLE_ADMIN_ID),
                "editor" => RoleId::new(ROLE_EDITOR_ID),
                "writer" => RoleId::new(ROLE_WRITER_ID),
                "viewer" => RoleId::new(ROLE_VIEWER_ID),
                custom => {
                    let uuid = custom
                        .parse()
                        .map_err(|e| format!("invalid role ID '{custom}': {e}"))?;
                    RoleId::new(uuid)
                }
            };
            role_ids.push(role_id);
        }
        registry.register(entry.token, service_id, role_ids);
    }

    Ok(registry)
}

fn parse_bool(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    )
}

fn parse_safety_policy(s: &str) -> SafetyPolicy {
    match s.trim().to_ascii_lowercase().as_str() {
        "destructive" | "allowdestructive" | "allow-destructive" | "allow_destructive" => {
            SafetyPolicy::AllowDestructive
        }
        _ => SafetyPolicy::AdditiveOnly,
    }
}

async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "Failed to listen for Ctrl+C signal");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::error!(error = %err, "Failed to install SIGTERM signal handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received SIGINT (Ctrl+C), starting graceful shutdown");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, starting graceful shutdown");
        }
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}
