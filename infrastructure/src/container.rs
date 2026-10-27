//! Composition root container wiring database persistence, application services, and HTTP state.

use std::sync::Arc;

use sqlx::PgPool;

use application::services::{
    AccessRequestsServiceImpl, DocumentsServiceImpl, SchemaServiceImpl, SystemConfigServiceImpl,
};
use domain::system::SystemContext;

use crate::api::state::{HttpState, ProductionHttpState};
use crate::auth::{
    OidcTokenValidator, ServiceTokenRegistry, SqlxAuthResolver, SqlxShadowUserRecorder,
};
use crate::health::SqlxHealthChecker;
use crate::persistence::naming::SchemaNaming;
use crate::persistence::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Sub-container aggregating SQLx repository adapters and database persistence machinery.
#[derive(Debug, Clone)]
pub struct PersistenceContainer {
    pub naming: &'static SchemaNaming,
    pub document_instance_repo: SqlxDocumentInstanceRepository,
    pub document_snapshot_repo: SqlxDocumentSnapshotRepository,
    pub role_repo: SqlxRoleRepository,
    pub assignment_repo: SqlxUserRoleAssignmentRepository,
    pub access_request_repo: SqlxAccessRequestRepository,
    pub shadow_user_recorder: SqlxShadowUserRecorder,
}

/// Central composition root container holding initialized services and adapters.
#[derive(Clone)]
pub struct AppContainer {
    pub context: &'static SystemContext,
    pub persistence: PersistenceContainer,
    pub auth_resolver: Arc<SqlxAuthResolver>,
    pub documents_service: Arc<
        DocumentsServiceImpl<SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository>,
    >,
    pub access_requests_service: Arc<
        AccessRequestsServiceImpl<
            SqlxAccessRequestRepository,
            SqlxUserRoleAssignmentRepository,
            SqlxRoleRepository,
        >,
    >,
    pub schema_service: Arc<SchemaServiceImpl>,
    pub system_config_service: Arc<SystemConfigServiceImpl>,
    pub health_checker: Arc<SqlxHealthChecker>,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl PersistenceContainer {
    /// Creates a new persistence container bound to the provided connection pool.
    #[must_use]
    pub fn new(pool: PgPool, context: &'static SystemContext) -> Self {
        let naming = SchemaNaming::from_registry(&context.schema);
        let document_instance_repo =
            SqlxDocumentInstanceRepository::new(pool.clone(), context, naming);
        let document_snapshot_repo =
            SqlxDocumentSnapshotRepository::new(pool.clone(), context, naming);
        let role_repo = SqlxRoleRepository::new(pool.clone());
        let assignment_repo = SqlxUserRoleAssignmentRepository::new(pool.clone());
        let access_request_repo = SqlxAccessRequestRepository::new(pool.clone());
        let shadow_user_recorder = SqlxShadowUserRecorder::new(pool);

        Self {
            naming,
            document_instance_repo,
            document_snapshot_repo,
            role_repo,
            assignment_repo,
            access_request_repo,
            shadow_user_recorder,
        }
    }
}

impl AppContainer {
    /// Constructs the central composition root from a database pool and startup context.
    #[must_use]
    pub fn from_pool_and_context(
        pool: PgPool,
        context: &'static SystemContext,
        oidc_validator: Arc<OidcTokenValidator>,
        service_tokens: Arc<ServiceTokenRegistry>,
    ) -> Self {
        let persistence = PersistenceContainer::new(pool.clone(), context);

        let documents_service = Arc::new(DocumentsServiceImpl::new_with_snapshots(
            persistence.document_instance_repo.clone(),
            persistence.document_snapshot_repo.clone(),
            context,
        ));

        let access_requests_service = Arc::new(AccessRequestsServiceImpl::new(
            persistence.access_request_repo.clone(),
            persistence.assignment_repo.clone(),
            persistence.role_repo.clone(),
        ));

        let schema_service = Arc::new(SchemaServiceImpl::new(context));
        let system_config_service = Arc::new(SystemConfigServiceImpl::new(context));
        let health_checker = Arc::new(SqlxHealthChecker::new(pool));

        let auth_resolver = Arc::new(SqlxAuthResolver::new(
            oidc_validator,
            service_tokens,
            persistence.access_request_repo.clone(),
            persistence.assignment_repo.clone(),
            persistence.role_repo.clone(),
            persistence.shadow_user_recorder.clone(),
        ));

        Self {
            context,
            persistence,
            auth_resolver,
            documents_service,
            access_requests_service,
            schema_service,
            system_config_service,
            health_checker,
        }
    }

    /// Projects the service layer into repository-free [`ProductionHttpState`].
    #[must_use]
    pub fn http_state(&self) -> ProductionHttpState {
        HttpState::new(
            self.context,
            Arc::clone(&self.auth_resolver),
            Arc::clone(&self.documents_service),
            Arc::clone(&self.access_requests_service),
            Arc::clone(&self.schema_service),
            Arc::clone(&self.system_config_service),
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

        let container = AppContainer::from_pool_and_context(
            pool,
            context,
            oidc_validator,
            service_tokens,
        );

        let state = container.http_state();
        assert_eq!(state.context.config.available_locales.len(), 1);
    }
}
