//! Generic REST API router assembly and route registration.

use axum::Router;
use axum::middleware::from_fn;
use axum::routing::{get, post};

use application::services::{
    AccessRequestsService, DocumentsService, SchemaService, SystemConfigService,
    UserDirectoryService,
};

use crate::api::handlers::{access_requests, content, health, schema, system, users};
use crate::api::middleware::{inject_problem_instance, propagate_request_id};
use crate::api::state::{
    HasAccessRequestsService, HasAuthResolver, HasDocumentsService, HasHealthChecker,
    HasSchemaService, HasSystemConfigService, HasUserDirectoryService, HttpState,
};
use crate::auth::AuthContextResolver;
use crate::health::HealthCheckService;

// ---------------------------------------------------------------------------
// Public Router Builders (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Builds the complete HTTP driving adapter REST router bound to `state`.
///
/// Registers all health probes, system configuration, schema metadata, access request,
/// user directory, and document content lifecycle routes with tracing and RFC 9457 middleware.
pub fn build_router<D, A, S, C, H, Auth, U>(state: HttpState<D, A, S, C, H, Auth, U>) -> Router
where
    D: DocumentsService,
    A: AccessRequestsService,
    S: SchemaService,
    C: SystemConfigService,
    H: HealthCheckService,
    Auth: AuthContextResolver,
    U: UserDirectoryService,
{
    Router::new()
        .merge(health_routes::<HttpState<D, A, S, C, H, Auth, U>>())
        .merge(system_routes::<HttpState<D, A, S, C, H, Auth, U>>())
        .merge(schema_routes::<HttpState<D, A, S, C, H, Auth, U>>())
        .merge(access_request_routes::<HttpState<D, A, S, C, H, Auth, U>>())
        .merge(user_directory_routes::<HttpState<D, A, S, C, H, Auth, U>>())
        .merge(content_routes::<HttpState<D, A, S, C, H, Auth, U>>())
        .layer(from_fn(inject_problem_instance))
        .layer(from_fn(propagate_request_id))
        .with_state(state)
}

/// Builds health and readiness probe routes (`/healthz`, `/ready`).
pub fn health_routes<St>() -> Router<St>
where
    St: HasHealthChecker,
{
    Router::new()
        .route("/healthz", get(health::healthz::<St>))
        .route("/ready", get(health::ready::<St>))
}

/// Builds system configuration and locale routes (`/api/v1/system/*`).
pub fn system_routes<St>() -> Router<St>
where
    St: HasSystemConfigService + HasAuthResolver,
{
    Router::new()
        .route("/api/v1/system/config", get(system::get_system_config::<St>))
        .route(
            "/api/v1/system/locales",
            get(system::get_system_locales::<St>),
        )
}

/// Builds schema definition routes (`/api/v1/schema/*` and `/api/v1/schemas/*`).
pub fn schema_routes<St>() -> Router<St>
where
    St: HasSchemaService + HasAuthResolver,
{
    Router::new()
        .route(
            "/api/v1/schema/document-types",
            get(schema::list_document_types::<St>),
        )
        .route(
            "/api/v1/schema/document-types/{id}",
            get(schema::get_document_type::<St>),
        )
        .route("/api/v1/schemas", get(schema::list_document_types::<St>))
        .route(
            "/api/v1/schemas/{id}",
            get(schema::get_document_type::<St>),
        )
}

/// Builds access request and onboarding routes (`/api/v1/auth/access-requests/*`).
pub fn access_request_routes<St>() -> Router<St>
where
    St: HasAccessRequestsService + HasAuthResolver,
{
    Router::new()
        .route(
            "/api/v1/auth/access-requests",
            post(access_requests::submit_access_request::<St>)
                .get(access_requests::list_pending_access_requests::<St>),
        )
        .route(
            "/api/v1/auth/access-requests/{id}",
            get(access_requests::get_access_request_by_id::<St>),
        )
        .route(
            "/api/v1/auth/access-requests/{id}/approve",
            post(access_requests::approve_access_request::<St>),
        )
        .route(
            "/api/v1/auth/access-requests/{id}/reject",
            post(access_requests::reject_access_request::<St>),
        )
}

/// Builds user directory and profile synchronization routes (`/api/v1/auth/users` and `/api/v1/auth/profile/sync`).
pub fn user_directory_routes<St>() -> Router<St>
where
    St: HasUserDirectoryService + HasAuthResolver,
{
    Router::new()
        .route("/api/v1/auth/users", get(users::list_users::<St>))
        .route("/api/v1/auth/profile/sync", post(users::sync_profile::<St>))
}

/// Builds document instance lifecycle and snapshot routes (`/api/v1/content/*`).
pub fn content_routes<St>() -> Router<St>
where
    St: HasDocumentsService + HasAuthResolver,
{
    Router::new()
        .route(
            "/api/v1/content/{type_id}",
            get(content::find_documents::<St>).post(content::create_document::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/admin/headers",
            get(content::list_admin_headers::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/admin-headers",
            get(content::list_admin_headers::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/{id}",
            get(content::find_document_by_id::<St>)
                .put(content::update_document::<St>)
                .delete(content::delete_document::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/{id}/publish",
            post(content::publish_document::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/{id}/unpublish",
            post(content::unpublish_document::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/{id}/discard-draft",
            post(content::discard_draft::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/{id}/snapshots",
            get(content::list_snapshots::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/{id}/snapshots/{revision}",
            get(content::get_snapshot::<St>),
        )
        .route(
            "/api/v1/content/{type_id}/{id}/snapshots/{revision}/restore",
            post(content::restore_snapshot::<St>),
        )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum_test::TestServer;

    use crate::health::AlwaysHealthyChecker;

    #[derive(Clone)]
    struct HealthState(Arc<AlwaysHealthyChecker>);

    impl HasHealthChecker for HealthState {
        type Health = AlwaysHealthyChecker;

        fn health_checker(&self) -> &Self::Health {
            &self.0
        }
    }

    #[test]
    fn test_health_routes_construction() {
        let state = HealthState(Arc::new(AlwaysHealthyChecker));
        let _router: Router = health_routes().with_state(state);
    }

    #[tokio::test]
    async fn test_health_routes_with_server() {
        let state = HealthState(Arc::new(AlwaysHealthyChecker));
        let router: Router = health_routes().with_state(state);
        let server = TestServer::new(router);

        let res_healthz = server.get("/healthz").await;
        res_healthz.assert_status_ok();
        res_healthz.assert_json(&serde_json::json!({"status": "healthy"}));

        let res_ready = server.get("/ready").await;
        res_ready.assert_status_ok();
        res_ready.assert_json(&serde_json::json!({"status": "healthy"}));
    }
}
