//! Generic composition root container for HTTP services and adapters.

use std::sync::Arc;

use sqlx::PgPool;

use application::services::{
    AccessRequestsServiceImpl, DocumentsServiceImpl, SchemaServiceImpl, SystemConfigServiceImpl,
    UserDirectoryServiceImpl,
};
use domain::system::SystemContext;
use luminair_persistence::health::SqlxHealthChecker;
use luminair_persistence::naming::SchemaNaming;
use luminair_persistence::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserProfileRepository, SqlxUserRoleAssignmentRepository,
};

use crate::auth::AuthContextResolver;
use crate::state::{HttpState, ProductionHttpState};

/// Central composition root container holding initialized services and adapters.
pub struct AppContainer<Auth> {
    pub context: &'static SystemContext,
    pub auth_resolver: Arc<Auth>,
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

impl<Auth: AuthContextResolver> AppContainer<Auth> {
    /// Constructs the central composition root from a database pool, static system context,
    /// and a concrete [`AuthContextResolver`].
    #[must_use]
    pub fn new(pool: PgPool, context: &'static SystemContext, auth_resolver: Arc<Auth>) -> Self {
        let naming = SchemaNaming::from_registry(&context.schema);
        let document_instance_repo = SqlxDocumentInstanceRepository::new(pool.clone(), naming);
        let document_snapshot_repo = SqlxDocumentSnapshotRepository::new(pool.clone(), naming);
        let role_repo = SqlxRoleRepository::new(pool.clone());
        let assignment_repo = SqlxUserRoleAssignmentRepository::new(pool.clone());
        let access_request_repo = SqlxAccessRequestRepository::new(pool.clone());
        let user_profile_repo = SqlxUserProfileRepository::new(pool.clone());

        let documents_service = Arc::new(DocumentsServiceImpl::new_with_snapshots(
            document_instance_repo,
            document_snapshot_repo,
            context,
        ));

        let access_requests_service = Arc::new(AccessRequestsServiceImpl::new(
            access_request_repo,
            assignment_repo,
            role_repo,
        ));

        let user_directory_service = Arc::new(UserDirectoryServiceImpl::new(user_profile_repo));

        let schema_service = Arc::new(SchemaServiceImpl::new(context));
        let system_config_service = Arc::new(SystemConfigServiceImpl::new(context));
        let health_checker = Arc::new(SqlxHealthChecker::new(pool));

        Self {
            context,
            auth_resolver,
            documents_service,
            access_requests_service,
            user_directory_service,
            schema_service,
            system_config_service,
            health_checker,
        }
    }

    /// Projects the service layer into repository-free [`HttpState`].
    #[must_use]
    pub fn http_state(&self) -> ProductionHttpState<Auth> {
        HttpState::new(
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
