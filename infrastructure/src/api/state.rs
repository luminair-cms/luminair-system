//! Generic HTTP state and interface-segregated state accessor traits.

use std::sync::Arc;

use application::services::{
    AccessRequestsService, AccessRequestsServiceImpl, DocumentsService, DocumentsServiceImpl,
    SchemaService, SchemaServiceImpl, SystemConfigService, SystemConfigServiceImpl,
};
use domain::system::SystemContext;

use crate::auth::{AuthContextResolver, SqlxAuthResolver};
use crate::health::{HealthCheckService, SqlxHealthChecker};
use crate::persistence::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};

// ---------------------------------------------------------------------------
// Interface-Segregated State Traits
// ---------------------------------------------------------------------------

/// Provides access to the static system context configuration and schema.
pub trait HasSystemContext: Clone + Send + Sync + 'static {
    fn system_context(&self) -> &'static SystemContext;
}

/// Provides access to the authentication and caller context resolver.
pub trait HasAuthResolver: HasSystemContext {
    type Auth: AuthContextResolver;

    fn auth_resolver(&self) -> &Self::Auth;
}

/// Provides access to the document instances application service.
pub trait HasDocumentsService: HasSystemContext {
    type Documents: DocumentsService;

    fn documents_service(&self) -> &Self::Documents;
}

/// Provides access to the access requests and onboarding application service.
pub trait HasAccessRequestsService: HasSystemContext {
    type AccessRequests: AccessRequestsService;

    fn access_requests_service(&self) -> &Self::AccessRequests;
}

/// Provides access to the schema metadata application service.
pub trait HasSchemaService: HasSystemContext {
    type Schema: SchemaService;

    fn schema_service(&self) -> &Self::Schema;
}

/// Provides access to the system configuration application service.
pub trait HasSystemConfigService: HasSystemContext {
    type SystemConfig: SystemConfigService;

    fn system_config_service(&self) -> &Self::SystemConfig;
}

/// Provides access to the system health and readiness checker.
pub trait HasHealthChecker: Clone + Send + Sync + 'static {
    type Health: HealthCheckService;

    fn health_checker(&self) -> &Self::Health;
}

// ---------------------------------------------------------------------------
// Types & Structs
// ---------------------------------------------------------------------------

/// Generic transport-agnostic state container shared across HTTP driving adapters.
///
/// Contains strictly application service traits, health checker, auth resolver,
/// and static system context—zero database pools or persistence repositories.
pub struct HttpState<
    D = DocumentsServiceImpl<SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository>,
    A = AccessRequestsServiceImpl<
        SqlxAccessRequestRepository,
        SqlxUserRoleAssignmentRepository,
        SqlxRoleRepository,
    >,
    S = SchemaServiceImpl,
    C = SystemConfigServiceImpl,
    H = SqlxHealthChecker,
    Auth = SqlxAuthResolver,
> {
    pub context: &'static SystemContext,
    pub auth_resolver: Arc<Auth>,
    pub documents_service: Arc<D>,
    pub access_requests_service: Arc<A>,
    pub schema_service: Arc<S>,
    pub system_config_service: Arc<C>,
    pub health_checker: Arc<H>,
}

/// Concrete production type alias binding `HttpState` to database-backed services.
pub type ProductionHttpState = HttpState<
    DocumentsServiceImpl<SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository>,
    AccessRequestsServiceImpl<
        SqlxAccessRequestRepository,
        SqlxUserRoleAssignmentRepository,
        SqlxRoleRepository,
    >,
    SchemaServiceImpl,
    SystemConfigServiceImpl,
    SqlxHealthChecker,
    SqlxAuthResolver,
>;

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl<D, A, S, C, H, Auth> HttpState<D, A, S, C, H, Auth> {
    /// Creates a new `HttpState` wrapping the provided application services.
    #[must_use]
    pub fn new(
        context: &'static SystemContext,
        auth_resolver: Arc<Auth>,
        documents_service: Arc<D>,
        access_requests_service: Arc<A>,
        schema_service: Arc<S>,
        system_config_service: Arc<C>,
        health_checker: Arc<H>,
    ) -> Self {
        Self {
            context,
            auth_resolver,
            documents_service,
            access_requests_service,
            schema_service,
            system_config_service,
            health_checker,
        }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl<D, A, S, C, H, Auth> Clone for HttpState<D, A, S, C, H, Auth> {
    fn clone(&self) -> Self {
        Self {
            context: self.context,
            auth_resolver: Arc::clone(&self.auth_resolver),
            documents_service: Arc::clone(&self.documents_service),
            access_requests_service: Arc::clone(&self.access_requests_service),
            schema_service: Arc::clone(&self.schema_service),
            system_config_service: Arc::clone(&self.system_config_service),
            health_checker: Arc::clone(&self.health_checker),
        }
    }
}

impl<D, A, S, C, H, Auth> HasSystemContext for HttpState<D, A, S, C, H, Auth>
where
    D: Send + Sync + 'static,
    A: Send + Sync + 'static,
    S: Send + Sync + 'static,
    C: Send + Sync + 'static,
    H: Send + Sync + 'static,
    Auth: Send + Sync + 'static,
{
    fn system_context(&self) -> &'static SystemContext {
        self.context
    }
}

impl<D, A, S, C, H, Auth> HasAuthResolver for HttpState<D, A, S, C, H, Auth>
where
    D: Send + Sync + 'static,
    A: Send + Sync + 'static,
    S: Send + Sync + 'static,
    C: Send + Sync + 'static,
    H: Send + Sync + 'static,
    Auth: AuthContextResolver,
{
    type Auth = Auth;

    fn auth_resolver(&self) -> &Self::Auth {
        self.auth_resolver.as_ref()
    }
}

impl<D, A, S, C, H, Auth> HasDocumentsService for HttpState<D, A, S, C, H, Auth>
where
    D: DocumentsService,
    A: Send + Sync + 'static,
    S: Send + Sync + 'static,
    C: Send + Sync + 'static,
    H: Send + Sync + 'static,
    Auth: Send + Sync + 'static,
{
    type Documents = D;

    fn documents_service(&self) -> &Self::Documents {
        self.documents_service.as_ref()
    }
}

impl<D, A, S, C, H, Auth> HasAccessRequestsService for HttpState<D, A, S, C, H, Auth>
where
    D: Send + Sync + 'static,
    A: AccessRequestsService,
    S: Send + Sync + 'static,
    C: Send + Sync + 'static,
    H: Send + Sync + 'static,
    Auth: Send + Sync + 'static,
{
    type AccessRequests = A;

    fn access_requests_service(&self) -> &Self::AccessRequests {
        self.access_requests_service.as_ref()
    }
}

impl<D, A, S, C, H, Auth> HasSchemaService for HttpState<D, A, S, C, H, Auth>
where
    D: Send + Sync + 'static,
    A: Send + Sync + 'static,
    S: SchemaService,
    C: Send + Sync + 'static,
    H: Send + Sync + 'static,
    Auth: Send + Sync + 'static,
{
    type Schema = S;

    fn schema_service(&self) -> &Self::Schema {
        self.schema_service.as_ref()
    }
}

impl<D, A, S, C, H, Auth> HasSystemConfigService for HttpState<D, A, S, C, H, Auth>
where
    D: Send + Sync + 'static,
    A: Send + Sync + 'static,
    S: Send + Sync + 'static,
    C: SystemConfigService,
    H: Send + Sync + 'static,
    Auth: Send + Sync + 'static,
{
    type SystemConfig = C;

    fn system_config_service(&self) -> &Self::SystemConfig {
        self.system_config_service.as_ref()
    }
}

impl<D, A, S, C, H, Auth> HasHealthChecker for HttpState<D, A, S, C, H, Auth>
where
    D: Send + Sync + 'static,
    A: Send + Sync + 'static,
    S: Send + Sync + 'static,
    C: Send + Sync + 'static,
    H: HealthCheckService,
    Auth: Send + Sync + 'static,
{
    type Health = H;

    fn health_checker(&self) -> &Self::Health {
        self.health_checker.as_ref()
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use application::commands::access_requests::{
        ApproveAccessRequestCommand, RejectAccessRequestCommand, SubmitAccessRequestCommand,
    };
    use application::commands::documents::{
        CreateDocumentCommand, DeleteDocumentCommand, DiscardDraftCommand, FindByIdCommand,
        FindDocumentsCommand, GetSnapshotCommand, ListAdminHeadersCommand, ListSnapshotsCommand,
        PublishDocumentCommand, RestoreSnapshotCommand, UnpublishDocumentCommand,
        UpdateDocumentCommand,
    };
    use application::context::CallerContext;
    use application::errors::ApplicationError;
    use domain::auth::access_request::{AccessRequest, AccessRequestId};
    use domain::auth::role::UserRoleAssignment;
    use domain::content::header::{
        DocumentHeader, DocumentSnapshot, SnapshotHeader,
    };
    use domain::content::instance::DocumentInstance;
    use domain::schema::{DocumentType, DocumentTypeId, SchemaRegistry};
    use domain::system::{LocaleId, SystemConfig, SystemConfigId};

    use crate::auth::{AuthError, AuthResolutionError, AuthenticatedUser};
    use crate::health::HealthStatus;

    // Explicitly non-Clone dummy structs to verify HttpState manual Clone works without Clone bounds
    struct NonCloneDocs;
    struct NonCloneAccess;
    struct NonCloneSchema;
    struct NonCloneConfig;
    struct NonCloneHealth;
    struct NonCloneAuth;

    impl DocumentsService for NonCloneDocs {
        async fn find(
            &self,
            _caller: &CallerContext,
            _cmd: FindDocumentsCommand,
        ) -> Result<(Vec<DocumentInstance>, u64), ApplicationError> {
            Ok((Vec::new(), 0))
        }

        async fn find_by_id(
            &self,
            _caller: &CallerContext,
            _cmd: FindByIdCommand,
        ) -> Result<Option<DocumentInstance>, ApplicationError> {
            Ok(None)
        }

        async fn create(
            &self,
            _caller: &CallerContext,
            _cmd: CreateDocumentCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn update(
            &self,
            _caller: &CallerContext,
            _cmd: UpdateDocumentCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn delete(
            &self,
            _caller: &CallerContext,
            _cmd: DeleteDocumentCommand,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn publish(
            &self,
            _caller: &CallerContext,
            _cmd: PublishDocumentCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn unpublish(
            &self,
            _caller: &CallerContext,
            _cmd: UnpublishDocumentCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn discard_draft(
            &self,
            _caller: &CallerContext,
            _cmd: DiscardDraftCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn restore_snapshot(
            &self,
            _caller: &CallerContext,
            _cmd: RestoreSnapshotCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn list_admin_headers(
            &self,
            _caller: &CallerContext,
            _cmd: ListAdminHeadersCommand,
        ) -> Result<(Vec<DocumentHeader>, u64), ApplicationError> {
            Ok((Vec::new(), 0))
        }

        async fn list_snapshots(
            &self,
            _caller: &CallerContext,
            _cmd: ListSnapshotsCommand,
        ) -> Result<Vec<SnapshotHeader>, ApplicationError> {
            Ok(Vec::new())
        }

        async fn get_snapshot(
            &self,
            _caller: &CallerContext,
            _cmd: GetSnapshotCommand,
        ) -> Result<DocumentSnapshot, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }
    }

    impl AccessRequestsService for NonCloneAccess {
        async fn submit(
            &self,
            _cmd: SubmitAccessRequestCommand,
        ) -> Result<AccessRequest, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn approve(
            &self,
            _caller: &CallerContext,
            _cmd: ApproveAccessRequestCommand,
        ) -> Result<Vec<UserRoleAssignment>, ApplicationError> {
            Ok(Vec::new())
        }

        async fn reject(
            &self,
            _caller: &CallerContext,
            _cmd: RejectAccessRequestCommand,
        ) -> Result<AccessRequest, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn get_by_id(
            &self,
            _caller: &CallerContext,
            _request_id: AccessRequestId,
        ) -> Result<AccessRequest, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }

        async fn list_pending(
            &self,
            _caller: &CallerContext,
        ) -> Result<Vec<AccessRequest>, ApplicationError> {
            Ok(Vec::new())
        }
    }

    impl SchemaService for NonCloneSchema {
        fn list_document_types(
            &self,
            _caller: &CallerContext,
        ) -> Result<Vec<DocumentType>, ApplicationError> {
            Ok(Vec::new())
        }

        fn get_document_type(
            &self,
            _caller: &CallerContext,
            _type_id: &DocumentTypeId,
        ) -> Result<DocumentType, ApplicationError> {
            Err(ApplicationError::Internal("test".into()))
        }
    }

    impl SystemConfigService for NonCloneConfig {
        fn get_config(&self) -> &SystemConfig {
            &fixture_context().config
        }
    }

    impl HealthCheckService for NonCloneHealth {
        async fn liveness(&self) -> HealthStatus {
            HealthStatus::Healthy
        }

        async fn readiness(&self) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    impl AuthContextResolver for NonCloneAuth {
        async fn resolve_user_identity(
            &self,
            _auth_header: Option<&str>,
        ) -> Result<AuthenticatedUser, AuthResolutionError> {
            Err(AuthError::MissingToken.into())
        }

        async fn resolve_caller(
            &self,
            _auth_header: Option<&str>,
        ) -> Result<CallerContext, AuthResolutionError> {
            Err(AuthError::MissingToken.into())
        }
    }

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

    #[test]
    fn test_http_state_clone_with_non_clone_services() {
        let context = fixture_context();
        let state = HttpState::new(
            context,
            Arc::new(NonCloneAuth),
            Arc::new(NonCloneDocs),
            Arc::new(NonCloneAccess),
            Arc::new(NonCloneSchema),
            Arc::new(NonCloneConfig),
            Arc::new(NonCloneHealth),
        );

        let cloned = state.clone();
        assert_eq!(cloned.system_context().config.available_locales.len(), 1);
    }

    #[tokio::test]
    async fn test_has_traits_accessors() {
        let context = fixture_context();
        let state = HttpState::new(
            context,
            Arc::new(NonCloneAuth),
            Arc::new(NonCloneDocs),
            Arc::new(NonCloneAccess),
            Arc::new(NonCloneSchema),
            Arc::new(NonCloneConfig),
            Arc::new(NonCloneHealth),
        );

        assert_eq!(state.system_context().config.available_locales.len(), 1);
        assert_eq!(
            state.health_checker().liveness().await,
            HealthStatus::Healthy
        );
        assert_eq!(
            state.system_config_service().default_locale().as_ref(),
            "en"
        );
    }
}
