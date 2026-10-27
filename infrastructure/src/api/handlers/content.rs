//! Content instance management, publication lifecycle, and revision snapshot route handlers.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use application::commands::{
    CreateDocumentCommand, DeleteDocumentCommand, DiscardDraftCommand, FindByIdCommand,
    FindDocumentsCommand, GetSnapshotCommand, ListAdminHeadersCommand, ListSnapshotsCommand,
    PublishDocumentCommand, RestoreSnapshotCommand, UnpublishDocumentCommand,
    UnpublishDraftResolution, UpdateDocumentCommand, VersionView,
};
use application::errors::ApplicationError;
use application::services::DocumentsService;

use crate::api::codec::{
    DocumentHeaderDto, DocumentInstanceDto, DocumentSnapshotDto, SnapshotHeaderDto,
    decode_document_payload,
};
use crate::api::envelope::{CreatedJson, JsonData, PagedJson};
use crate::api::error::ApiError;
use crate::api::extractors::{
    AdminHeadersQuery, FindByIdQuery, FindDocumentsQuery, parse_instance_id, parse_populate_param,
    resolve_document_type,
};
use crate::api::state::{HasAuthResolver, HasDocumentsService};
use crate::auth::Caller;

// ---------------------------------------------------------------------------
// Request Payload Structs (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Request payload for creating a document instance (`POST /api/v1/content/{type_id}`).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDocumentPayload {
    /// Document attribute field values.
    #[serde(default)]
    pub fields: serde_json::Map<String, serde_json::Value>,
    /// Relational links or mutation directives.
    pub relations: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Request payload for updating an existing document (`PUT /api/v1/content/{type_id}/{id}`).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDocumentPayload {
    /// Updated document attribute field values.
    #[serde(default)]
    pub fields: serde_json::Map<String, serde_json::Value>,
    /// Relational mutations.
    pub relations: Option<serde_json::Map<String, serde_json::Value>>,
    /// Expected version for optimistic concurrency control.
    #[serde(alias = "expected_version")]
    pub expected_version: Option<u32>,
}

/// Optional payload for publishing a draft document (`POST .../publish`).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishDocumentPayload {
    /// Expected working draft version for optimistic concurrency control.
    #[serde(alias = "expected_version")]
    pub expected_version: Option<u32>,
}

/// Optional payload for taking a published document offline (`POST .../unpublish`).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnpublishDocumentPayload {
    /// Strategy for resolving pending working draft edits.
    #[serde(alias = "draft_resolution")]
    pub draft_resolution: Option<UnpublishDraftResolution>,
}

/// Optional payload for restoring a snapshot revision (`POST .../restore`).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSnapshotPayload {
    /// When true, allows restoring even if uncommitted working draft edits exist.
    #[serde(default, alias = "discard_pending_edits")]
    pub discard_pending_edits: bool,
}

/// Query parameter for relational attribute population (`?populate=author,tags`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopulateQuery {
    /// Comma-separated attribute IDs to populate, or `*` for all relations.
    pub populate: Option<String>,
}

// ---------------------------------------------------------------------------
// Public Handler Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Queries a paginated list of documents (`GET /api/v1/content/{type_id}`).
pub async fn find_documents<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path(type_id): Path<String>,
    Query(query): Query<FindDocumentsQuery>,
) -> Result<PagedJson<DocumentInstanceDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let pagination = query.pagination();
    let view = query.parse_view(VersionView::Published)?;
    let populate = query.parse_populate(doc_type)?;

    let cmd = FindDocumentsCommand::new(&doc_type.id, pagination)
        .with_view(view)
        .with_populate(populate);

    let (instances, total) = state.documents_service().find(&caller, cmd).await?;
    let dtos = instances.into_iter().map(DocumentInstanceDto::from).collect();
    Ok(PagedJson::new(dtos, pagination, total))
}

/// Lists lightweight document headers for Admin Studio views (`GET /api/v1/content/{type_id}/admin/headers`).
pub async fn list_admin_headers<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path(type_id): Path<String>,
    Query(query): Query<AdminHeadersQuery>,
) -> Result<PagedJson<DocumentHeaderDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let pagination = query.pagination();
    let mut cmd = ListAdminHeadersCommand::new(&doc_type.id, pagination);
    if let Some(status) = query.status {
        cmd = cmd.with_status_filter(status);
    }

    let (headers, total) = state.documents_service().list_admin_headers(&caller, cmd).await?;
    let dtos = headers.into_iter().map(DocumentHeaderDto::from).collect();
    Ok(PagedJson::new(dtos, pagination, total))
}

/// Fetches a single document instance by identifier (`GET /api/v1/content/{type_id}/{id}`).
pub async fn find_document_by_id<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id)): Path<(String, String)>,
    Query(query): Query<FindByIdQuery>,
) -> Result<JsonData<DocumentInstanceDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let view = query.parse_view(VersionView::Published)?;
    let populate = query.parse_populate(doc_type)?;

    let cmd = FindByIdCommand::new(&doc_type.id, id)
        .with_view(view)
        .with_populate(populate);

    let maybe_doc = state.documents_service().find_by_id(&caller, cmd).await?;
    let doc = maybe_doc.ok_or_else(|| {
        ApiError::from(ApplicationError::NotFound {
            entity: "DocumentInstance",
            id: raw_id,
        })
    })?;

    Ok(JsonData::new(DocumentInstanceDto::from(doc)))
}

/// Creates a new document instance (`POST /api/v1/content/{type_id}`).
pub async fn create_document<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path(type_id): Path<String>,
    Query(query): Query<PopulateQuery>,
    Json(payload): Json<CreateDocumentPayload>,
) -> Result<CreatedJson<DocumentInstanceDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let populate = parse_populate_param(query.populate.as_deref(), doc_type)?;
    let (fields, relations) = decode_document_payload(doc_type, payload.fields, payload.relations)?;

    let cmd = CreateDocumentCommand::new(&doc_type.id, fields)
        .with_relations(relations)
        .with_populate(populate);

    let instance = state.documents_service().create(&caller, cmd).await?;
    Ok(CreatedJson::new(DocumentInstanceDto::from(instance)))
}

/// Updates an existing document instance (`PUT /api/v1/content/{type_id}/{id}`).
pub async fn update_document<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id)): Path<(String, String)>,
    Query(query): Query<PopulateQuery>,
    Json(payload): Json<UpdateDocumentPayload>,
) -> Result<JsonData<DocumentInstanceDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let populate = parse_populate_param(query.populate.as_deref(), doc_type)?;
    let (fields, relations) = decode_document_payload(doc_type, payload.fields, payload.relations)?;

    let mut cmd = UpdateDocumentCommand::new(id, &doc_type.id, fields)
        .with_relations(relations)
        .with_populate(populate);
    if let Some(expected_version) = payload.expected_version {
        cmd = cmd.with_expected_version(expected_version);
    }

    let instance = state.documents_service().update(&caller, cmd).await?;
    Ok(JsonData::new(DocumentInstanceDto::from(instance)))
}

/// Deletes a document instance (`DELETE /api/v1/content/{type_id}/{id}`).
pub async fn delete_document<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let cmd = DeleteDocumentCommand::new(id, &doc_type.id);

    state.documents_service().delete(&caller, cmd).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Publishes a draft document (`POST /api/v1/content/{type_id}/{id}/publish`).
pub async fn publish_document<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id)): Path<(String, String)>,
    payload: Option<Json<PublishDocumentPayload>>,
) -> Result<JsonData<DocumentInstanceDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let mut cmd = PublishDocumentCommand::new(id, &doc_type.id);
    if let Some(Json(p)) = payload
        && let Some(v) = p.expected_version
    {
        cmd = cmd.with_expected_version(v);
    }

    let instance = state.documents_service().publish(&caller, cmd).await?;
    Ok(JsonData::new(DocumentInstanceDto::from(instance)))
}

/// Takes a published document offline (`POST /api/v1/content/{type_id}/{id}/unpublish`).
pub async fn unpublish_document<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id)): Path<(String, String)>,
    payload: Option<Json<UnpublishDocumentPayload>>,
) -> Result<JsonData<DocumentInstanceDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let resolution = payload
        .and_then(|Json(p)| p.draft_resolution)
        .unwrap_or_default();
    let cmd = UnpublishDocumentCommand::new(id, &doc_type.id).with_draft_resolution(resolution);

    let instance = state.documents_service().unpublish(&caller, cmd).await?;
    Ok(JsonData::new(DocumentInstanceDto::from(instance)))
}

/// Discards uncommitted working draft edits (`POST /api/v1/content/{type_id}/{id}/discard-draft`).
pub async fn discard_draft<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id)): Path<(String, String)>,
) -> Result<JsonData<DocumentInstanceDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let cmd = DiscardDraftCommand::new(id, &doc_type.id);

    let instance = state.documents_service().discard_draft(&caller, cmd).await?;
    Ok(JsonData::new(DocumentInstanceDto::from(instance)))
}

/// Lists historical revision snapshot headers for a document (`GET /api/v1/content/{type_id}/{id}/snapshots`).
pub async fn list_snapshots<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id)): Path<(String, String)>,
) -> Result<JsonData<Vec<SnapshotHeaderDto>>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let cmd = ListSnapshotsCommand::new(&doc_type.id, id);

    let snapshots = state.documents_service().list_snapshots(&caller, cmd).await?;
    let dtos = snapshots.into_iter().map(SnapshotHeaderDto::from).collect();
    Ok(JsonData::new(dtos))
}

/// Retrieves a specific historical revision snapshot (`GET /api/v1/content/{type_id}/{id}/snapshots/{revision}`).
pub async fn get_snapshot<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id, revision)): Path<(String, String, u32)>,
) -> Result<JsonData<DocumentSnapshotDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let cmd = GetSnapshotCommand::new(&doc_type.id, id, revision);

    let snapshot = state.documents_service().get_snapshot(&caller, cmd).await?;
    Ok(JsonData::new(DocumentSnapshotDto::from(snapshot)))
}

/// Restores a historical revision snapshot into the working draft (`POST /api/v1/content/{type_id}/{id}/snapshots/{revision}/restore`).
pub async fn restore_snapshot<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path((type_id, raw_id, revision)): Path<(String, String, u32)>,
    payload: Option<Json<RestoreSnapshotPayload>>,
) -> Result<JsonData<DocumentInstanceDto>, ApiError>
where
    St: HasDocumentsService + HasAuthResolver,
{
    let doc_type = resolve_document_type(state.system_context(), &type_id)?;
    let id = parse_instance_id(&raw_id)?;
    let discard_pending_edits = payload.is_some_and(|Json(p)| p.discard_pending_edits);

    let mut cmd = RestoreSnapshotCommand::new(id, &doc_type.id, revision);
    if discard_pending_edits {
        cmd = cmd.with_discard_pending_edits();
    }

    let instance = state.documents_service().restore_snapshot(&caller, cmd).await?;
    Ok(JsonData::new(DocumentInstanceDto::from(instance)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use chrono::Utc;
    use domain::auth::user::{ActorId, UserId};
    use domain::content::header::{DocumentHeader, DocumentSnapshot, DocumentStatus, SnapshotHeader, SnapshotId, SnapshotReason};
    use domain::content::instance::{AuditTrail, DocumentContent, DocumentInstance, DocumentInstanceId, PublicationState};
    use domain::schema::documents::{DocumentKind, DocumentType, DocumentTypeInfo, DocumentTypeOptions};
    use domain::schema::types::{FieldType, PrimitiveType};
    use domain::schema::attributes::FieldDefinition;
    use domain::schema::SchemaRegistry;
    use domain::system::locale::LocaleId;
    use domain::system::{SystemConfig, SystemConfigId, SystemContext};
    use uuid::Uuid;

    use crate::api::state::HasSystemContext;
    use crate::auth::{AuthContextResolver, AuthResolutionError, AuthenticatedUser};

    #[derive(Clone)]
    struct MockContentState {
        context: &'static SystemContext,
        resolver: Arc<MockAuthResolver>,
        service: Arc<MockDocService>,
    }

    impl HasSystemContext for MockContentState {
        fn system_context(&self) -> &'static SystemContext {
            self.context
        }
    }

    impl HasAuthResolver for MockContentState {
        type Auth = MockAuthResolver;

        fn auth_resolver(&self) -> &Self::Auth {
            &self.resolver
        }
    }

    impl HasDocumentsService for MockContentState {
        type Documents = MockDocService;

        fn documents_service(&self) -> &Self::Documents {
            &self.service
        }
    }

    struct MockAuthResolver;

    impl AuthContextResolver for MockAuthResolver {
        async fn resolve_user_identity(
            &self,
            _auth_header: Option<&str>,
        ) -> Result<AuthenticatedUser, AuthResolutionError> {
            let user_id = UserId::try_new("usr_test").expect("valid user");
            Ok(AuthenticatedUser::new(user_id, None, None))
        }

        async fn resolve_caller(
            &self,
            _auth_header: Option<&str>,
        ) -> Result<application::context::CallerContext, AuthResolutionError> {
            let user_id = UserId::try_new("usr_test").expect("valid user");
            Ok(application::context::CallerContext::new(
                ActorId::User(user_id),
                vec![],
            ))
        }
    }

    struct MockDocService {
        sample_instance: DocumentInstance,
    }

    impl DocumentsService for MockDocService {
        async fn find(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: FindDocumentsCommand,
        ) -> Result<(Vec<DocumentInstance>, u64), ApplicationError> {
            Ok((vec![self.sample_instance.clone()], 1))
        }

        async fn find_by_id(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: FindByIdCommand,
        ) -> Result<Option<DocumentInstance>, ApplicationError> {
            Ok(Some(self.sample_instance.clone()))
        }

        async fn create(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: CreateDocumentCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Ok(self.sample_instance.clone())
        }

        async fn update(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: UpdateDocumentCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Ok(self.sample_instance.clone())
        }

        async fn delete(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: DeleteDocumentCommand,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn publish(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: PublishDocumentCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Ok(self.sample_instance.clone())
        }

        async fn unpublish(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: UnpublishDocumentCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Ok(self.sample_instance.clone())
        }

        async fn discard_draft(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: DiscardDraftCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Ok(self.sample_instance.clone())
        }

        async fn restore_snapshot(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: RestoreSnapshotCommand,
        ) -> Result<DocumentInstance, ApplicationError> {
            Ok(self.sample_instance.clone())
        }

        async fn list_admin_headers(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: ListAdminHeadersCommand,
        ) -> Result<(Vec<DocumentHeader>, u64), ApplicationError> {
            let header = DocumentHeader {
                id: self.sample_instance.id,
                document_type_id: self.sample_instance.document_type_id.clone(),
                display_values: indexmap::IndexMap::new(),
                display_title: "Sample Article".to_string(),
                relational_display_target: None,
                status: DocumentStatus::Draft,
                slug: None,
                revision: None,
                draft_version: Some(1),
                created_at: self.sample_instance.audit.created_at,
                created_by: self.sample_instance.audit.created_by.clone(),
                updated_at: self.sample_instance.audit.updated_at,
                updated_by: self.sample_instance.audit.updated_by.clone(),
                published_at: None,
                published_by: None,
            };
            Ok((vec![header], 1))
        }

        async fn list_snapshots(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: ListSnapshotsCommand,
        ) -> Result<Vec<SnapshotHeader>, ApplicationError> {
            let header = SnapshotHeader {
                id: SnapshotId::new(Uuid::now_v7()),
                document_type_id: self.sample_instance.document_type_id.clone(),
                document_id: self.sample_instance.id,
                revision: 1,
                created_at: Utc::now(),
                created_by: ActorId::User(UserId::try_new("usr_test").unwrap()),
                reason: SnapshotReason::Publish,
            };
            Ok(vec![header])
        }

        async fn get_snapshot(
            &self,
            _caller: &application::context::CallerContext,
            _cmd: GetSnapshotCommand,
        ) -> Result<DocumentSnapshot, ApplicationError> {
            let snapshot = DocumentSnapshot {
                id: SnapshotId::new(Uuid::now_v7()),
                document_type_id: self.sample_instance.document_type_id.clone(),
                document_id: self.sample_instance.id,
                revision: 1,
                fields: HashMap::new(),
                relations: HashMap::new(),
                created_at: Utc::now(),
                created_by: ActorId::User(UserId::try_new("usr_test").unwrap()),
                reason: SnapshotReason::Publish,
            };
            Ok(snapshot)
        }
    }

    fn test_system_context() -> &'static SystemContext {
        let id = domain::schema::DocumentTypeId::try_new("articles").expect("valid id");
        let title_attr = domain::schema::AttributeId::try_new("title").expect("valid id");
        let mut fields = indexmap::IndexSet::new();
        fields.insert(FieldDefinition {
            id: title_attr,
            field_type: FieldType::Primitive(PrimitiveType::Text),
            unique: true,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        let dt = DocumentType {
            id: id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: domain::common::DisplayName::try_new("Article").unwrap(),
                singular_name: id.clone(),
                plural_name: id,
                description: None,
                display_fields: vec![],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
            relations: indexmap::IndexSet::new(),
        };
        let registry = SchemaRegistry::new(vec![dt]);
        let en = LocaleId::try_new("en").expect("valid locale");
        let config = SystemConfig::new(
            SystemConfigId::new(Uuid::now_v7()),
            vec![en.clone()],
            en,
        )
        .expect("valid config");
        Box::leak(Box::new(SystemContext::new(registry, config)))
    }

    fn sample_document_instance() -> DocumentInstance {
        let type_id = domain::schema::DocumentTypeId::try_new("articles").unwrap();
        let doc_id = DocumentInstanceId::new(Uuid::now_v7());
        let actor = ActorId::User(UserId::try_new("usr_test").unwrap());
        DocumentInstance {
            id: doc_id,
            document_type_id: type_id,
            content: DocumentContent::new(HashMap::new(), PublicationState::Draft),
            relations: HashMap::new(),
            populated_relations: HashMap::new(),
            audit: AuditTrail {
                created_at: Utc::now(),
                created_by: actor.clone(),
                updated_at: Utc::now(),
                updated_by: actor,
                version: 1,
            },
        }
    }

    #[tokio::test]
    async fn test_find_documents_handler() {
        let inst = sample_document_instance();
        let state = MockContentState {
            context: test_system_context(),
            resolver: Arc::new(MockAuthResolver),
            service: Arc::new(MockDocService { sample_instance: inst }),
        };
        let caller = Caller(application::context::CallerContext::new(
            ActorId::User(UserId::try_new("usr_test").unwrap()),
            vec![],
        ));

        let res = find_documents(
            State(state),
            caller,
            Path("articles".to_string()),
            Query(FindDocumentsQuery::default()),
        )
        .await
        .expect("find should succeed");

        assert_eq!(res.data.len(), 1);
        assert_eq!(res.meta.total, 1);
    }

    #[tokio::test]
    async fn test_delete_document_handler() {
        let inst = sample_document_instance();
        let raw_id = inst.id.to_string();
        let state = MockContentState {
            context: test_system_context(),
            resolver: Arc::new(MockAuthResolver),
            service: Arc::new(MockDocService { sample_instance: inst }),
        };
        let caller = Caller(application::context::CallerContext::new(
            ActorId::User(UserId::try_new("usr_test").unwrap()),
            vec![],
        ));

        let status = delete_document(
            State(state),
            caller,
            Path(("articles".to_string(), raw_id)),
        )
        .await
        .expect("delete should succeed");

        assert_eq!(status, StatusCode::NO_CONTENT);
    }
}
