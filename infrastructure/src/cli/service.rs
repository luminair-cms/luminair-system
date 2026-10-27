//! Execution mode worker for running the long-lived HTTP REST API server (`service` mode).

use std::sync::Arc;

use sqlx::PgPool;
use tokio::net::TcpListener;

use domain::auth::role::RoleId;
use domain::auth::user::ServiceId;
use domain::system::SystemContext;

use super::config::CliConfig;
use super::error::CliError;
use super::report::{CliOutcome, ServiceReport};
use crate::api::routes::build_router;
use crate::auth::{OidcTokenValidator, ServiceTokenRegistry};
use crate::container::AppContainer;
use crate::persistence::migration::{
    ROLE_ADMIN_ID, ROLE_EDITOR_ID, ROLE_VIEWER_ID, ROLE_WRITER_ID, run_static_migrations,
    sync_dynamic_schemas,
};
use crate::persistence::{PersistenceError, create_connection_pool};

// ---------------------------------------------------------------------------
// Public Orchestrator (SLAP Level 0)
// ---------------------------------------------------------------------------

/// Orchestrates the startup, request handling, and graceful shutdown of the HTTP REST service.
///
/// # Errors
/// Returns [`CliError::Config`] if required database or auth parameters are missing or invalid.
/// Returns [`CliError::Persistence`] if database connection or auto-migration fails.
/// Returns [`CliError::Server`] if listener binding or HTTP server execution fails.
pub async fn execute_service_mode(
    config: &CliConfig,
    context: &'static SystemContext,
) -> Result<CliOutcome, CliError> {
    let pool = setup_database(config, context).await?;
    let (oidc_validator, service_tokens) = build_auth_machinery(config)?;
    let container =
        AppContainer::from_pool_and_context(pool, context, oidc_validator, service_tokens);
    let router = build_router(container.http_state());
    let listener = bind_listener(config).await?;
    let local_addr = listener.local_addr().map_err(|source| CliError::Io {
        path: std::path::PathBuf::from(format!("{}:{}", config.http_host, config.http_port)),
        source,
    })?;

    tracing::info!(
        host = %config.http_host,
        port = %local_addr.port(),
        doc_types = %context.schema.iterate().count(),
        "Starting Luminair HTTP Service"
    );

    serve_with_graceful_shutdown(listener, router).await?;

    Ok(CliOutcome::Service(ServiceReport {
        bind_address: local_addr.to_string(),
        status: "Shutdown cleanly".to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Private Helpers (SLAP Level 1)
// ---------------------------------------------------------------------------

async fn setup_database(
    config: &CliConfig,
    context: &'static SystemContext,
) -> Result<PgPool, CliError> {
    let db_url = config.database_url.as_deref().ok_or_else(|| {
        CliError::Config("database_url is required for 'service' mode".to_string())
    })?;

    let pool = create_connection_pool(db_url, config.max_db_connections).await?;

    if config.auto_migrate {
        tracing::info!("Running auto-migrations prior to starting HTTP listener...");
        run_static_migrations(&pool)
            .await
            .map_err(PersistenceError::from)?;
        sync_dynamic_schemas(&pool, &context.schema, config.safety_policy).await?;
        tracing::info!("Auto-migrations successfully applied");
    }

    Ok(pool)
}

fn build_auth_machinery(
    config: &CliConfig,
) -> Result<(Arc<OidcTokenValidator>, Arc<ServiceTokenRegistry>), CliError> {
    let oidc_validator = build_oidc_validator(config)?;
    let service_token_registry = build_service_token_registry(config)?;
    Ok((oidc_validator, Arc::new(service_token_registry)))
}

fn build_oidc_validator(config: &CliConfig) -> Result<Arc<OidcTokenValidator>, CliError> {
    if let Some(ref secret) = config.oidc_jwt_secret {
        Ok(Arc::new(OidcTokenValidator::new_hs256(
            secret.as_bytes(),
            config.oidc_issuer.clone(),
            config.oidc_audience.clone(),
        )))
    } else if let Some(ref pem) = config.oidc_jwt_public_key_pem {
        let validator = OidcTokenValidator::from_rsa_pem(
            pem.as_bytes(),
            config.oidc_issuer.clone(),
            config.oidc_audience.clone(),
        )
        .map_err(|e| CliError::Config(format!("invalid OIDC RS256 public key: {e}")))?;
        Ok(Arc::new(validator))
    } else {
        tracing::warn!("No OIDC credentials configured; running in insecure disabled auth mode");
        Ok(Arc::new(OidcTokenValidator::disabled()))
    }
}

fn build_service_token_registry(config: &CliConfig) -> Result<ServiceTokenRegistry, CliError> {
    let mut registry = ServiceTokenRegistry::new();
    for entry in &config.service_tokens {
        let service_id = ServiceId::try_new(&entry.service_id).map_err(|e| {
            CliError::Config(format!("invalid service ID '{}': {e}", entry.service_id))
        })?;
        let role_ids = entry
            .roles
            .iter()
            .map(|r| parse_role_id(r))
            .collect::<Result<Vec<RoleId>, CliError>>()?;

        registry.register(entry.token.clone(), service_id, role_ids);
    }
    Ok(registry)
}

fn parse_role_id(role_str: &str) -> Result<RoleId, CliError> {
    match role_str.trim().to_ascii_lowercase().as_str() {
        "admin" => Ok(RoleId::new(ROLE_ADMIN_ID)),
        "editor" => Ok(RoleId::new(ROLE_EDITOR_ID)),
        "writer" => Ok(RoleId::new(ROLE_WRITER_ID)),
        "viewer" => Ok(RoleId::new(ROLE_VIEWER_ID)),
        raw_uuid => {
            let u = uuid::Uuid::parse_str(raw_uuid)
                .map_err(|e| CliError::Config(format!("invalid role ID '{role_str}': {e}")))?;
            Ok(RoleId::new(u))
        }
    }
}

async fn bind_listener(config: &CliConfig) -> Result<TcpListener, CliError> {
    let addr = format!("{}:{}", config.http_host, config.http_port);
    TcpListener::bind(&addr).await.map_err(|source| CliError::Io {
        path: std::path::PathBuf::from(addr),
        source,
    })
}

async fn serve_with_graceful_shutdown(
    listener: TcpListener,
    router: axum::Router,
) -> Result<(), CliError> {
    axum::serve(listener, router)
        .with_graceful_shutdown(wait_for_shutdown_signal())
        .await
        .map_err(|e| CliError::Server(e.to_string()))
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
