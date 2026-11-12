//! Composition root container wiring database persistence, application services, and HTTP state.

use std::sync::Arc;

use sqlx::PgPool;

use application::services::{
    AccessRequestsServiceImpl, DocumentsServiceImpl, SchemaServiceImpl, SystemConfigServiceImpl,
    UserDirectoryServiceImpl,
};
use domain::system::SystemContext;
use luminair_auth::{OidcTokenValidator, ServiceTokenRegistry, SqlxAuthResolver};
use luminair_http::container::AppContainer as HttpAppContainer;
use luminair_http::state::ProductionHttpState;
use luminair_persistence::health::SqlxHealthChecker;
use luminair_persistence::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserProfileRepository, SqlxUserRoleAssignmentRepository,
};

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Central composition root container holding initialized services and adapters.
pub(crate) struct AppContainer {
    pub context: &'static SystemContext,
    pub auth_resolver: Arc<SqlxAuthResolver>,
    pub documents_service:
        Arc<DocumentsServiceImpl<SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository>>,
    pub access_requests_service: Arc<
        AccessRequestsServiceImpl<
            SqlxAccessRequestRepository,
            SqlxUserRoleAssignmentRepository,
            SqlxRoleRepository,
        >,
    >,
    pub user_directory_service: Arc<UserDirectoryServiceImpl<SqlxUserProfileRepository>>,
    pub schema_service: Arc<SchemaServiceImpl>,
    pub system_config_service: Arc<SystemConfigServiceImpl>,
    pub health_checker: Arc<SqlxHealthChecker>,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl AppContainer {
    /// Constructs the central composition root from a database pool and startup context.
    #[must_use]
    pub(crate) fn from_pool_and_context(
        pool: PgPool,
        context: &'static SystemContext,
        oidc_validator: Arc<OidcTokenValidator>,
        service_tokens: Arc<ServiceTokenRegistry>,
    ) -> Self {
        let auth_resolver = Arc::new(SqlxAuthResolver::from_pool(
            pool.clone(),
            oidc_validator,
            service_tokens,
        ));
        let inner = HttpAppContainer::new(pool, context, auth_resolver);

        Self {
            context: inner.context,
            auth_resolver: inner.auth_resolver,
            documents_service: inner.documents_service,
            access_requests_service: inner.access_requests_service,
            user_directory_service: inner.user_directory_service,
            schema_service: inner.schema_service,
            system_config_service: inner.system_config_service,
            health_checker: inner.health_checker,
        }
    }

    /// Projects the service layer into repository-free [`ProductionHttpState`].
    #[must_use]
    pub(crate) fn http_state(&self) -> ProductionHttpState<SqlxAuthResolver> {
        luminair_http::state::HttpState::new(
            self.context,
            Arc::clone(&self.auth_resolver),
            Arc::clone(&self.documents_service),
            Arc::clone(&self.access_requests_service),
            Arc::clone(&self.schema_service),
            Arc::clone(&self.system_config_service),
            Arc::clone(&self.user_directory_service),
            Arc::clone(&self.health_checker),
        )
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::schema::SchemaRegistry;
    use domain::system::{LocaleId, SystemConfig, SystemConfigId};

    fn fixture_context() -> &'static SystemContext {
        let registry = SchemaRegistry::new(vec![]);
        let locale = LocaleId::try_new("en").expect("valid locale");
        let config = SystemConfig::new(
            SystemConfigId::new(uuid::Uuid::now_v7()),
            vec![locale.clone()],
            locale,
        )
        .expect("valid config");
        Box::leak(Box::new(SystemContext::new(registry, config)))
    }

    #[tokio::test]
    async fn test_container_assembly_and_http_state_projection() {
        let context = fixture_context();
        let pool = PgPool::connect_lazy("postgres://localhost:5432/test_db")
            .expect("lazy pool initialization");

        let oidc_validator = Arc::new(OidcTokenValidator::new_hs256(
            b"secret-key-that-is-at-least-32-bytes-long",
            None,
            None,
        ));
        let service_tokens = Arc::new(ServiceTokenRegistry::new());

        let container =
            AppContainer::from_pool_and_context(pool, context, oidc_validator, service_tokens);

        let state = container.http_state();
        assert_eq!(state.context.config.available_locales.len(), 1);
    }
}
