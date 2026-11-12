//! AWS Lambda custom runtime bootstrap entry point.

use std::path::Path;
use std::sync::Arc;

use lambda_http::{Error, run};
use luminair_auth_cognito_edge::CognitoEdgeAuthResolver;
use luminair_http::container::AppContainer;
use luminair_http::routes::build_router;
use luminair_persistence::create_connection_pool;
use luminair_schema_loader::load_system_context_from_dir;

#[tokio::main]
async fn main() -> Result<(), Error> {
    init_tracing();

    // 1. INIT Phase (executed once per Lambda sandbox)
    let schema_dir = std::env::var("LUMINAIR_SCHEMA_DIR")
        .unwrap_or_else(|_| "/var/task/schemas".into());
    let context = load_system_context_from_dir(Path::new(&schema_dir))
        .expect("schema directory must be valid at cold start");

    let db_url = std::env::var("LUMINAIR_DATABASE_URL")
        .expect("LUMINAIR_DATABASE_URL is required");

    // Serverless connection pool: max_connections = 1
    let pool = create_connection_pool(&db_url, 1)
        .await
        .expect("failed to connect to Aurora DSQL");

    let auth_resolver = Arc::new(CognitoEdgeAuthResolver::new(pool.clone()));
    let container = AppContainer::new(pool, context, auth_resolver);
    let router = build_router(container.http_state());

    // 2. Invocation Loop
    run(router).await
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}
