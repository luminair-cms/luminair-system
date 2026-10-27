//! Schema definition and document type metadata route handlers.

use axum::extract::{Path, State};

use application::errors::ApplicationError;
use application::services::SchemaService;
use domain::schema::DocumentTypeId;

use crate::api::codec::DocumentTypeDto;
use crate::api::envelope::JsonData;
use crate::api::error::ApiError;
use crate::api::state::{HasAuthResolver, HasSchemaService};
use crate::auth::Caller;

// ---------------------------------------------------------------------------
// Public Handler Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Lists all registered document types authorized for the caller (`GET /api/v1/schema/document-types`).
pub async fn list_document_types<St>(
    State(state): State<St>,
    Caller(caller): Caller,
) -> Result<JsonData<Vec<DocumentTypeDto>>, ApiError>
where
    St: HasSchemaService + HasAuthResolver,
{
    let types = state.schema_service().list_document_types(&caller)?;
    let dtos = types.into_iter().map(DocumentTypeDto::from).collect();
    Ok(JsonData::new(dtos))
}

/// Retrieves schema definition for a specific document type by ID (`GET /api/v1/schema/document-types/{id}`).
pub async fn get_document_type<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path(raw_id): Path<String>,
) -> Result<JsonData<DocumentTypeDto>, ApiError>
where
    St: HasSchemaService + HasAuthResolver,
{
    let type_id = DocumentTypeId::try_new(&raw_id).map_err(|_| {
        ApiError::from(ApplicationError::NotFound {
            entity: "DocumentType",
            id: raw_id,
        })
    })?;

    let dt = state.schema_service().get_document_type(&caller, &type_id)?;
    Ok(JsonData::new(DocumentTypeDto::from(dt)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use application::context::CallerContext;
    use domain::auth::user::{ActorId, UserId};
    use domain::common::DisplayName;
    use domain::schema::documents::{DocumentKind, DocumentType, DocumentTypeInfo, DocumentTypeOptions};
    use domain::schema::SchemaRegistry;
    use domain::system::locale::LocaleId;
    use domain::system::{SystemConfig, SystemConfigId, SystemContext};
    use indexmap::IndexSet;

    use crate::api::state::HasSystemContext;
    use crate::auth::{AuthContextResolver, AuthResolutionError, AuthenticatedUser};

    #[derive(Clone)]
    struct MockSchemaState {
        context: &'static SystemContext,
        resolver: Arc<MockAuthResolver>,
        service: Arc<MockSchemaService>,
    }

    impl HasSystemContext for MockSchemaState {
        fn system_context(&self) -> &'static SystemContext {
            self.context
        }
    }

    impl HasAuthResolver for MockSchemaState {
        type Auth = MockAuthResolver;

        fn auth_resolver(&self) -> &Self::Auth {
            &self.resolver
        }
    }

    impl HasSchemaService for MockSchemaState {
        type Schema = MockSchemaService;

        fn schema_service(&self) -> &Self::Schema {
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
        ) -> Result<CallerContext, AuthResolutionError> {
            let user_id = UserId::try_new("usr_test").expect("valid user");
            Ok(CallerContext::new(ActorId::User(user_id), vec![]))
        }
    }

    struct MockSchemaService {
        doc_type: DocumentType,
    }

    impl SchemaService for MockSchemaService {
        fn list_document_types(
            &self,
            _caller: &CallerContext,
        ) -> Result<Vec<DocumentType>, ApplicationError> {
            Ok(vec![self.doc_type.clone()])
        }

        fn get_document_type(
            &self,
            _caller: &CallerContext,
            type_id: &DocumentTypeId,
        ) -> Result<DocumentType, ApplicationError> {
            if type_id == &self.doc_type.id {
                Ok(self.doc_type.clone())
            } else {
                Err(ApplicationError::NotFound {
                    entity: "DocumentType",
                    id: type_id.as_ref().to_string(),
                })
            }
        }
    }

    fn test_system_context() -> &'static SystemContext {
        let registry = SchemaRegistry::new(vec![]);
        let en = LocaleId::try_new("en").expect("valid locale");
        let config = SystemConfig::new(
            SystemConfigId::new(uuid::Uuid::now_v7()),
            vec![en.clone()],
            en,
        )
        .expect("valid config");
        Box::leak(Box::new(SystemContext::new(registry, config)))
    }

    fn test_doc_type() -> DocumentType {
        let id = DocumentTypeId::try_new("articles").expect("valid id");
        DocumentType {
            id: id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Article").unwrap(),
                singular_name: id.clone(),
                plural_name: id,
                description: None,
                display_fields: vec![],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: IndexSet::new(),
            relations: IndexSet::new(),
        }
    }

    #[tokio::test]
    async fn test_list_document_types_success() {
        let doc_type = test_doc_type();
        let state = MockSchemaState {
            context: test_system_context(),
            resolver: Arc::new(MockAuthResolver),
            service: Arc::new(MockSchemaService { doc_type }),
        };
        let caller = Caller(CallerContext::new(
            ActorId::User(UserId::try_new("usr_test").unwrap()),
            vec![],
        ));

        let res = list_document_types(State(state), caller)
            .await
            .expect("list should succeed");
        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id.as_ref(), "articles");
    }

    #[tokio::test]
    async fn test_get_document_type_not_found() {
        let doc_type = test_doc_type();
        let state = MockSchemaState {
            context: test_system_context(),
            resolver: Arc::new(MockAuthResolver),
            service: Arc::new(MockSchemaService { doc_type }),
        };
        let caller = Caller(CallerContext::new(
            ActorId::User(UserId::try_new("usr_test").unwrap()),
            vec![],
        ));

        let err = get_document_type(State(state), caller, Path("nonexistent".to_string()))
            .await
            .expect_err("should return not found");
        assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);
    }
}
