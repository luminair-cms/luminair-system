//! Schema application service.

use domain::auth::{Permission, PermissionCheck};
use domain::errors::DomainError;
use domain::schema::{DocumentType, DocumentTypeId};
use domain::system::SystemContext;

use crate::context::CallerContext;
use crate::errors::ApplicationError;

/// Port trait defining use cases for schema and document type definitions.
pub trait SchemaService: Send + Sync + 'static {
    /// Lists all document types that the caller is authorized to view.
    ///
    /// # Errors
    /// Returns [`ApplicationError::Unauthorized`] if the caller has no schema permissions.
    fn list_document_types(
        &self,
        caller: &CallerContext,
    ) -> Result<Vec<DocumentType>, ApplicationError>;

    /// Fetches a specific document type definition by its unique identifier.
    ///
    /// # Errors
    /// Returns [`ApplicationError::NotFound`] if the document type does not exist,
    /// or [`ApplicationError::Unauthorized`] if caller lacks permission.
    fn get_document_type(
        &self,
        caller: &CallerContext,
        type_id: &DocumentTypeId,
    ) -> Result<DocumentType, ApplicationError>;
}

/// Implementation of `SchemaService` operating over `SystemContext`.
#[derive(Debug, Clone)]
pub struct SchemaServiceImpl {
    pub context: &'static SystemContext,
}

impl SchemaServiceImpl {
    #[must_use]
    pub fn new(context: &'static SystemContext) -> Self {
        Self { context }
    }
}

impl SchemaService for SchemaServiceImpl {
    fn list_document_types(
        &self,
        caller: &CallerContext,
    ) -> Result<Vec<DocumentType>, ApplicationError> {
        let types: Vec<DocumentType> = self
            .context
            .all_types()
            .filter(|dt| {
                caller.can(&PermissionCheck::read_document(&dt.id), None)
                    || caller.can(&PermissionCheck::ManageSchema, None)
            })
            .cloned()
            .collect();

        // If caller cannot read any types and lacks ManageSchema/wildcard, deny
        if types.is_empty()
            && !caller.can(&PermissionCheck::ManageSchema, None)
            && !caller.can(&PermissionCheck::ReadDocument(None), None)
        {
            return Err(ApplicationError::Unauthorized {
                actor: caller.actor.clone(),
                action: Permission::ReadDocument(None),
            });
        }

        Ok(types)
    }

    fn get_document_type(
        &self,
        caller: &CallerContext,
        type_id: &DocumentTypeId,
    ) -> Result<DocumentType, ApplicationError> {
        if !caller.can(&PermissionCheck::read_document(type_id), None)
            && !caller.can(&PermissionCheck::ManageSchema, None)
        {
            return Err(ApplicationError::Unauthorized {
                actor: caller.actor.clone(),
                action: Permission::ReadDocument(Some(type_id.clone())),
            });
        }

        self.context.find_type(type_id).cloned().ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(type_id.clone()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::auth::{ActorId, Role, RoleId, UserId};
    use domain::schema::SchemaRegistry;
    use domain::test_support::{DocumentTypeBuilder, fixture_system_config, text_field};

    fn setup_schema_fixture() -> (
        SchemaServiceImpl,
        &'static DocumentType,
        &'static DocumentType,
    ) {
        let article_type = DocumentTypeBuilder::collection("article")
            .with_title("Articles")
            .with_field(text_field("title"))
            .build();
        let page_type = DocumentTypeBuilder::collection("page")
            .with_title("Pages")
            .with_field(text_field("title"))
            .build();

        let schema = SchemaRegistry::new(vec![article_type.clone(), page_type.clone()]);
        let config = fixture_system_config();
        let context: &'static SystemContext =
            Box::leak(Box::new(SystemContext::new(schema, config)));

        let leaked_article = context.find_type(&article_type.id).unwrap();
        let leaked_page = context.find_type(&page_type.id).unwrap();

        (SchemaServiceImpl::new(context), leaked_article, leaked_page)
    }

    #[test]
    fn test_system_caller_lists_all_document_types() {
        let (service, article, page) = setup_schema_fixture();
        let caller = CallerContext::system();

        let types = service.list_document_types(&caller).unwrap();
        assert_eq!(types.len(), 2);
        assert!(types.iter().any(|t| t.id == article.id));
        assert!(types.iter().any(|t| t.id == page.id));
    }

    #[test]
    fn test_scoped_caller_lists_only_authorized_types() {
        let (service, article, _) = setup_schema_fixture();
        let user_id = UserId::try_new("editor").unwrap();
        let role = Role {
            id: RoleId::new(uuid::Uuid::now_v7()),
            name: "ArticleEditor".into(),
            description: None,
            permissions: vec![Permission::ReadDocument(Some(article.id.clone()))],
        };
        let caller = CallerContext::new(ActorId::User(user_id), vec![role]);

        let types = service.list_document_types(&caller).unwrap();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].id, article.id);
    }

    #[test]
    fn test_unauthorized_caller_denied_listing() {
        let (service, _, _) = setup_schema_fixture();
        let user_id = UserId::try_new("unauthorized").unwrap();
        let caller = CallerContext::new(ActorId::User(user_id), vec![]);

        let res = service.list_document_types(&caller);
        assert!(matches!(res, Err(ApplicationError::Unauthorized { .. })));
    }

    #[test]
    fn test_get_document_type_success_and_not_found() {
        let (service, article, _) = setup_schema_fixture();
        let caller = CallerContext::system();

        let dt = service.get_document_type(&caller, &article.id).unwrap();
        assert_eq!(dt.id, article.id);

        let unknown_id = DocumentTypeId::try_new("unknown").unwrap();
        let res = service.get_document_type(&caller, &unknown_id);
        assert!(matches!(
            res,
            Err(ApplicationError::Domain(DomainError::DocumentTypeNotFound(
                _
            )))
        ));
    }
}
