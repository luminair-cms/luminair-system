//! Document instances application service.

use std::collections::{HashMap, HashSet};
use std::future::Future;

use chrono::Utc;
use domain::auth::Permission;
use domain::content::{
    DocumentInstance, DocumentInstanceId, DocumentInstanceRepository, RelationMap, validate_content,
};
use domain::errors::DomainError;
use domain::schema::{AttributeId, DocumentKind, DocumentType, DocumentTypeId, RelationType};
use domain::system::SystemContext;

use crate::commands::documents::*;
use crate::context::CallerContext;
use crate::errors::ApplicationError;

/// Port trait defining use cases for document instances.
pub trait DocumentsService: Send + Sync + 'static {
    /// Queries a paginated list of documents with sequential count and optional relation enrichment.
    fn find(
        &self,
        caller: &CallerContext,
        cmd: FindDocumentsCommand,
    ) -> impl Future<Output = Result<(Vec<DocumentInstance>, u64), ApplicationError>> + Send;

    /// Fetches a single document instance by identifier with optional relation enrichment.
    fn find_by_id(
        &self,
        caller: &CallerContext,
        cmd: FindByIdCommand,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, ApplicationError>> + Send;

    /// Creates a new draft document instance, enforcing singleton limits and schema validation.
    fn create(
        &self,
        caller: &CallerContext,
        cmd: CreateDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Updates field values of an existing document instance.
    fn update(
        &self,
        caller: &CallerContext,
        cmd: UpdateDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Deletes a document instance by identifier.
    fn delete(
        &self,
        caller: &CallerContext,
        cmd: DeleteDocumentCommand,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// Publishes a draft document instance.
    fn publish(
        &self,
        caller: &CallerContext,
        cmd: PublishDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Reverts a published document instance back to draft state.
    fn unpublish(
        &self,
        caller: &CallerContext,
        cmd: UnpublishDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;
}

/// Generic implementation of `DocumentsService` monomorphized over repository adapters.
pub struct DocumentsServiceImpl<R> {
    pub instance_repo: R,
    pub context: &'static SystemContext,
}

impl<R> DocumentsServiceImpl<R>
where
    R: DocumentInstanceRepository + 'static,
{
    pub fn new(instance_repo: R, context: &'static SystemContext) -> Self {
        Self {
            instance_repo,
            context,
        }
    }

    /// Two-phase batch relation enrichment:
    /// 1. Fetches related instances for all parent instances in a single batch query.
    /// 2. Stitches relations in-memory to each corresponding document instance.
    pub async fn enrich(
        &self,
        type_id: &'static DocumentTypeId,
        populate: &[AttributeId],
        instances: Vec<DocumentInstance>,
    ) -> Result<Vec<DocumentInstance>, ApplicationError> {
        if populate.is_empty() || instances.is_empty() {
            return Ok(instances);
        }

        let parent_ids: Vec<DocumentInstanceId> = instances.iter().map(|inst| inst.id).collect();

        // Batch fetch relations for all parent instances in one query
        let mut relation_map: RelationMap = self
            .instance_repo
            .fetch_relations(type_id, populate, &parent_ids)
            .await?;

        // Attach populated relations in-memory
        let enriched = instances
            .into_iter()
            .map(|inst| {
                let mut per_doc_relations = HashMap::new();
                for attr in populate {
                    if let Some(by_parent) = relation_map.get_mut(attr)
                        && let Some(related_items) = by_parent.remove(&inst.id)
                    {
                        per_doc_relations.insert(attr.clone(), related_items);
                    }
                }
                inst.with_populated_relations(per_doc_relations)
            })
            .collect();

        Ok(enriched)
    }

    /// Validates relational actions against schema registry invariants and applies them to the document instance.
    pub fn validate_and_apply_relations(
        &self,
        doc_type: &DocumentType,
        instance: &mut DocumentInstance,
        relations: HashMap<AttributeId, RelationAction>,
    ) -> Result<(), ApplicationError> {
        for (attr, action) in relations {
            let rel = doc_type.relations.get(&attr)
                .ok_or_else(|| {
                    ApplicationError::Validation(vec![format!(
                        "relation attribute '{attr}' is not defined for document type '{}'",
                        doc_type.id
                    )])
                })?;

            if !rel.relation_type.is_owning() {
                return Err(ApplicationError::Validation(vec![format!(
                    "relation attribute '{attr}' is an inverse-side relation; mutate from owner type '{:?}'",
                    rel.relation_type
                )]));
            }

            let is_has_one = rel.relation_type == RelationType::HasOne;

            match action {
                RelationAction::Set(target_ids) => {
                    let unique: HashSet<_> = target_ids.iter().collect();
                    if is_has_one && unique.len() > 1 {
                        return Err(ApplicationError::Validation(vec![format!(
                            "relation '{attr}' is HasOne but received {} target IDs",
                            unique.len()
                        )]));
                    }
                    instance.set_relations(attr, target_ids);
                }
                RelationAction::Connect(target_ids) => {
                    if is_has_one {
                        let mut unique: HashSet<_> = target_ids.iter().collect();
                        if let Some(existing) = instance.relations.get(&attr) {
                            for r in existing {
                                unique.insert(&r.target_instance_id);
                            }
                        }
                        if unique.len() > 1 {
                            return Err(ApplicationError::Validation(vec![format!(
                                "relation '{attr}' is HasOne and cannot have multiple targets connected"
                            )]));
                        }
                    }
                    instance.connect_relations(attr, target_ids);
                }
                RelationAction::Disconnect(target_ids) => {
                    instance.disconnect_relations(&attr, &target_ids);
                }
                RelationAction::Unset => {
                    instance.unset_relations(&attr);
                }
            }
        }

        Ok(())
    }
}

impl<R> DocumentsService for DocumentsServiceImpl<R>
where
    R: DocumentInstanceRepository + 'static,
{
    async fn find(
        &self,
        caller: &CallerContext,
        cmd: FindDocumentsCommand,
    ) -> Result<(Vec<DocumentInstance>, u64), ApplicationError> {
        caller.check_permission(
            &Permission::ReadDocument(Some(cmd.document_type.clone())),
            None,
        )?;

        self.context.find_type(cmd.document_type).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(cmd.document_type.clone()))
        })?;

        // Sequential fetch for MVP
        let page = self
            .instance_repo
            .find_by_type(cmd.document_type, cmd.pagination, cmd.filters.clone())
            .await?;
        let count = self
            .instance_repo
            .count(cmd.document_type, cmd.filters)
            .await?;

        // Two-phase batch relation enrichment
        let enriched = self
            .enrich(
                cmd.document_type,
                cmd.populate.as_deref().unwrap_or_default(),
                page.items,
            )
            .await?;

        Ok((enriched, count))
    }

    async fn find_by_id(
        &self,
        caller: &CallerContext,
        cmd: FindByIdCommand,
    ) -> Result<Option<DocumentInstance>, ApplicationError> {
        let instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?;

        match instance {
            Some(inst) => {
                caller.check_permission(
                    &Permission::ReadDocument(Some(inst.document_type_id.clone())),
                    Some(&inst),
                )?;
                let mut enriched = self
                    .enrich(
                        cmd.document_type,
                        cmd.populate.as_deref().unwrap_or_default(),
                        vec![inst],
                    )
                    .await?;
                Ok(enriched.pop())
            }
            None => Ok(None),
        }
    }

    async fn create(
        &self,
        caller: &CallerContext,
        cmd: CreateDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        caller.check_permission(
            &Permission::CreateDocument(Some(cmd.document_type.clone())),
            None,
        )?;

        let doc_type = self.context.find_type(cmd.document_type).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(cmd.document_type.clone()))
        })?;

        // Singleton Guard (ADR-002 Option C)
        if doc_type.kind == DocumentKind::SingleType
            && self
                .instance_repo
                .exists_for_type(cmd.document_type)
                .await?
        {
            return Err(ApplicationError::Domain(
                DomainError::SingleTypeAlreadyExists(cmd.document_type.clone()),
            ));
        }

        let mut instance = DocumentInstance::new(
            cmd.document_type.clone(),
            Some(caller.user_id.clone()),
            Utc::now(),
        );
        instance.content.fields = cmd.fields;

        // Schema constraint and required field validation
        if let Err(errs) = validate_content(doc_type, &instance.content, &self.context.config) {
            let messages: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
            return Err(ApplicationError::Validation(messages));
        }

        // Apply and validate relational mutations
        self.validate_and_apply_relations(doc_type, &mut instance, cmd.relations)?;

        self.instance_repo.save(&instance).await?;

        let populate = cmd.populate.as_deref().unwrap_or_default();
        if !populate.is_empty() {
            let mut enriched = self
                .enrich(cmd.document_type, populate, vec![instance])
                .await?;
            let doc = enriched
                .pop()
                .ok_or_else(|| ApplicationError::Internal("enriched instance missing from batch result".into()))?;
            Ok(doc)
        } else {
            Ok(instance)
        }
    }

    async fn update(
        &self,
        caller: &CallerContext,
        cmd: UpdateDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let mut instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::UpdateDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        // Apply updated fields (supports partial PATCH: omitted fields are retained,
        // while explicit ContentValue::Null clears/resets optional field values).
        for (attr, val) in cmd.fields {
            instance.content.fields.insert(attr, val);
        }

        let doc_type = self.context.find_type(cmd.document_type).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(cmd.document_type.clone()))
        })?;

        instance.touch(Some(caller.user_id.clone()), Utc::now());

        // Revalidate against schema constraints
        if let Err(errs) = validate_content(doc_type, &instance.content, &self.context.config) {
            let messages: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
            return Err(ApplicationError::Validation(messages));
        }

        // Apply and validate relational mutations
        self.validate_and_apply_relations(doc_type, &mut instance, cmd.relations)?;

        self.instance_repo.save(&instance).await?;

        let populate = cmd.populate.as_deref().unwrap_or_default();
        if !populate.is_empty() {
            let mut enriched = self
                .enrich(cmd.document_type, populate, vec![instance])
                .await?;
            let doc = enriched
                .pop()
                .ok_or_else(|| ApplicationError::Internal("enriched instance missing from batch result".into()))?;
            Ok(doc)
        } else {
            Ok(instance)
        }
    }

    async fn delete(
        &self,
        caller: &CallerContext,
        cmd: DeleteDocumentCommand,
    ) -> Result<(), ApplicationError> {
        let instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::DeleteDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        self.instance_repo
            .delete(cmd.document_type, cmd.document_instance_id)
            .await?;
        Ok(())
    }

    async fn publish(
        &self,
        caller: &CallerContext,
        cmd: PublishDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let mut instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::PublishDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        let doc_type = self.context.find_type(cmd.document_type).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(cmd.document_type.clone()))
        })?;

        // Draft-and-publish guard (R7): only types that support publish/unpublish can be published
        if !doc_type.options.draft_and_publish {
            return Err(ApplicationError::Conflict(format!(
                "document type '{}' does not support draft-and-publish workflow",
                doc_type.info.title
            )));
        }

        instance.publish(Some(caller.user_id.clone()), Utc::now())?;
        self.instance_repo.save(&instance).await?;

        Ok(instance)
    }

    async fn unpublish(
        &self,
        caller: &CallerContext,
        cmd: UnpublishDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let mut instance = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(cmd.document_instance_id),
            ))?;

        caller.check_permission(
            &Permission::PublishDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        let doc_type = self.context.find_type(cmd.document_type).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(cmd.document_type.clone()))
        })?;

        // Draft-and-publish guard (R7): only types that support publish/unpublish can be unpublished
        if !doc_type.options.draft_and_publish {
            return Err(ApplicationError::Conflict(format!(
                "document type '{}' does not support draft-and-publish workflow",
                doc_type.info.title
            )));
        }

        instance.unpublish(Some(caller.user_id.clone()), Utc::now())?;
        self.instance_repo.save(&instance).await?;

        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::auth::UserId;
    use domain::content::{
        ContentValue, DomainValue, Pagination, PrimitiveValue, PublicationState,
    };
    use domain::schema::{DocumentType, RelationDefinition, RelationType, SchemaRegistry};
    use domain::system::SystemContext;
    use domain::test_support::{fixture_system_config, text_field, DocumentTypeBuilder};
    use uuid::Uuid;

    use crate::test_support::FakeDocumentInstanceRepository;

    fn make_test_fixture(
        kind: DocumentKind,
    ) -> (
        DocumentsServiceImpl<FakeDocumentInstanceRepository>,
        &'static DocumentType,
        CallerContext,
        AttributeId,
    ) {
        let title_attr = AttributeId::try_new("title").unwrap();
        let builder = match kind {
            DocumentKind::Collection => DocumentTypeBuilder::collection("article"),
            DocumentKind::SingleType => DocumentTypeBuilder::single("article"),
        };
        let doc_type = builder
            .with_title("Articles")
            .with_field(text_field("title"))
            .build();

        let type_id = doc_type.id.clone();
        let config = fixture_system_config();
        let schema = SchemaRegistry::new(vec![doc_type]);
        let context: &'static SystemContext =
            Box::leak(Box::new(SystemContext::new(schema, config)));
        let leaked_doc_type = context.find_type(&type_id).unwrap();

        let instance_repo = FakeDocumentInstanceRepository::new();
        let service = DocumentsServiceImpl::new(instance_repo, context);

        let caller = CallerContext::system();
        (service, leaked_doc_type, caller, title_attr)
    }

    #[tokio::test]
    async fn test_create_and_find_by_id() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Hello Rust".into(),
            ))),
        );

        let created = service
            .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
            .await
            .expect("create success");

        let fetched = service
            .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, created.id))
            .await
            .expect("find success")
            .expect("found document");

        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.document_type_id, doc_type.id);
    }

    #[tokio::test]
    async fn test_create_single_type_guard_blocks_duplicate() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::SingleType);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Header".into(),
            ))),
        );

        // First creation succeeds
        service
            .create(
                &caller,
                CreateDocumentCommand::new(&doc_type.id, fields.clone()),
            )
            .await
            .expect("first create succeeds");

        // Second creation must fail with SingleTypeAlreadyExists
        let second = service
            .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
            .await;

        assert!(matches!(
            second,
            Err(ApplicationError::Domain(DomainError::SingleTypeAlreadyExists(id))) if id == doc_type.id
        ));
    }

    #[tokio::test]
    async fn test_find_with_batch_relation_enrichment() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Post 1".into(),
            ))),
        );

        let inst = service
            .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
            .await
            .unwrap();

        // Setup relation data in fake repo
        let rel_attr = AttributeId::try_new("tags").unwrap();
        let tag_type_id = DocumentTypeId::try_new("tag").unwrap();
        let related_tag = DocumentInstance::new(tag_type_id, None, Utc::now());

        service.instance_repo.add_relation_data(
            rel_attr.clone(),
            inst.id,
            vec![related_tag.clone()],
        );

        // Query with populate
        let cmd = FindDocumentsCommand::new(&doc_type.id, Pagination::default())
            .with_populate(vec![rel_attr.clone()]);

        let (items, count) = service.find(&caller, cmd).await.expect("find ok");
        assert_eq!(count, 1);
        assert_eq!(items.len(), 1);

        let doc = &items[0];
        let populated_tags = doc
            .populated_relations
            .get(&rel_attr)
            .expect("tags populated");
        assert_eq!(populated_tags.len(), 1);
        assert_eq!(populated_tags[0].id, related_tag.id);
    }

    #[tokio::test]
    async fn test_update_touches_audit_and_bumps_version() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Old Title".into(),
            ))),
        );

        let created = service
            .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
            .await
            .unwrap();
        assert_eq!(created.audit.version, 1);

        let mut update_fields = HashMap::new();
        update_fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "New Title".into(),
            ))),
        );

        let updated = service
            .update(
                &caller,
                UpdateDocumentCommand::new(created.id, &doc_type.id, update_fields),
            )
            .await
            .expect("update success");

        assert_eq!(updated.audit.version, 2);
    }

    #[tokio::test]
    async fn test_publish_and_unpublish_lifecycle() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Live News".into(),
            ))),
        );

        let created = service
            .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
            .await
            .unwrap();

        // Publish
        let published = service
            .publish(
                &caller,
                PublishDocumentCommand::new(created.id, &doc_type.id),
            )
            .await
            .expect("publish success");

        assert_eq!(published.id, created.id);
        assert!(matches!(
            published.content.publication_state,
            PublicationState::Published { revision: 1, .. }
        ));

        // Unpublish
        let unpublished = service
            .unpublish(
                &caller,
                UnpublishDocumentCommand::new(created.id, &doc_type.id),
            )
            .await
            .expect("unpublish success");

        assert!(matches!(
            unpublished.content.publication_state,
            PublicationState::Draft {
                last_published_revision: Some(1)
            }
        ));
        assert_eq!(unpublished.audit.updated_by, Some(caller.user_id));
    }

    #[tokio::test]
    async fn test_unauthorized_user_denied() {
        let (service, doc_type, _, title_attr) = make_test_fixture(DocumentKind::Collection);

        let unauthorized_caller = CallerContext::new(
            UserId::try_new("unauth_user").unwrap(),
            vec![], // no roles
        );

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Restricted".into(),
            ))),
        );

        let res = service
            .create(
                &unauthorized_caller,
                CreateDocumentCommand::new(&doc_type.id, fields),
            )
            .await;

        assert!(matches!(res, Err(ApplicationError::Unauthorized { .. })));
    }

    #[tokio::test]
    async fn test_publish_blocked_when_draft_and_publish_disabled() {
        let doc_type = DocumentTypeBuilder::collection("simple")
            .with_title("Simple")
            .with_draft_and_publish(false)
            .with_field(text_field("title").required(false))
            .build();

        let type_id = doc_type.id.clone();
        let config = fixture_system_config();
        let schema = SchemaRegistry::new(vec![doc_type]);
        let context: &'static SystemContext =
            Box::leak(Box::new(SystemContext::new(schema, config)));
        let leaked_doc_type = context.find_type(&type_id).unwrap();

        let instance_repo = FakeDocumentInstanceRepository::new();
        let service = DocumentsServiceImpl::new(instance_repo, context);
        let caller = CallerContext::system();

        let created = service
            .create(
                &caller,
                CreateDocumentCommand::new(&leaked_doc_type.id, HashMap::new()),
            )
            .await
            .expect("create succeeds");

        let publish_result = service
            .publish(
                &caller,
                PublishDocumentCommand::new(created.id, &leaked_doc_type.id),
            )
            .await;

        assert!(
            matches!(publish_result, Err(ApplicationError::Conflict(_))),
            "expected Conflict error when draft_and_publish=false, got: {:?}",
            publish_result
        );
    }

    #[tokio::test]
    async fn test_delete_removes_instance() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Collection);

        let mut fields = HashMap::new();
        fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "ToDelete".into(),
            ))),
        );
        let created = service
            .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
            .await
            .expect("create succeeds");

        service
            .delete(
                &caller,
                DeleteDocumentCommand::new(created.id, &doc_type.id),
            )
            .await
            .expect("delete succeeds");

        // After delete: instance is gone
        let found = service
            .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, created.id))
            .await
            .expect("find_by_id returns Ok");
        assert!(found.is_none(), "instance should be gone after delete");
    }

    fn make_relational_fixture() -> (
        DocumentsServiceImpl<FakeDocumentInstanceRepository>,
        &'static DocumentType,
        &'static DocumentType,
        CallerContext,
        AttributeId,
        AttributeId,
        AttributeId,
    ) {
        let article_type_id = DocumentTypeId::try_new("article").unwrap();
        let tag_type_id = DocumentTypeId::try_new("tag").unwrap();
        let cat_type_id = DocumentTypeId::try_new("category").unwrap();

        let title_attr = AttributeId::try_new("title").unwrap();
        let tags_attr = AttributeId::try_new("tags").unwrap();
        let cat_attr = AttributeId::try_new("category").unwrap();

        let tag_type = DocumentTypeBuilder::collection("tag")
            .with_title("Tags")
            .with_draft_and_publish(false)
            .with_field(text_field("name"))
            .build();

        let cat_type = DocumentTypeBuilder::collection("category")
            .with_title("Categories")
            .with_draft_and_publish(false)
            .with_field(text_field("name"))
            .build();

        let tags_rel = RelationDefinition {
            id: tags_attr.clone(),
            relation_type: RelationType::HasMany,
            target: tag_type_id.clone(),
        };

        let cat_rel = RelationDefinition {
            id: cat_attr.clone(),
            relation_type: RelationType::HasOne,
            target: cat_type_id,
        };

        let article_type = DocumentTypeBuilder::collection("article")
            .with_title("Articles")
            .with_draft_and_publish(true)
            .with_field(text_field("title"))
            .with_relation(tags_rel)
            .with_relation(cat_rel)
            .build();

        let config = fixture_system_config();
        let schema = SchemaRegistry::new(vec![article_type, tag_type, cat_type]);
        let context: &'static SystemContext =
            Box::leak(Box::new(SystemContext::new(schema, config)));
        let leaked_article = context.find_type(&article_type_id).unwrap();
        let leaked_tag = context.find_type(&tag_type_id).unwrap();

        let instance_repo = FakeDocumentInstanceRepository::new();
        let service = DocumentsServiceImpl::new(instance_repo, context);
        let caller = CallerContext::system();

        (
            service,
            leaked_article,
            leaked_tag,
            caller,
            title_attr,
            tags_attr,
            cat_attr,
        )
    }

    #[tokio::test]
    async fn test_create_with_relation_connect_and_populate() {
        let (service, article_type, tag_type, caller, title_attr, tags_attr, _) =
            make_relational_fixture();

        // 1. Create a tag first
        let mut tag_fields = HashMap::new();
        tag_fields.insert(
            AttributeId::try_new("name").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Rust".into()))),
        );
        let tag = service
            .create(
                &caller,
                CreateDocumentCommand::new(&tag_type.id, tag_fields),
            )
            .await
            .unwrap();

        // 2. Create article connecting to the tag with ?populate=tags
        let mut article_fields = HashMap::new();
        article_fields.insert(
            title_attr,
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "My Post".into(),
            ))),
        );

        let mut relations = HashMap::new();
        relations.insert(tags_attr.clone(), RelationAction::Connect(vec![tag.id]));

        let cmd = CreateDocumentCommand::new(&article_type.id, article_fields)
            .with_relations(relations)
            .with_populate(vec![tags_attr.clone()]);

        let created_article = service.create(&caller, cmd).await.expect("create success");

        // Verify resolved relations in instance.relations
        let rel_list = created_article
            .relations
            .get(&tags_attr)
            .expect("tags present in relations");
        assert_eq!(rel_list.len(), 1);
        assert_eq!(rel_list[0].target_instance_id, tag.id);

        // Verify populated relations in instance.populated_relations (Read-After-Write)
        let populated_list = created_article
            .populated_relations
            .get(&tags_attr)
            .expect("tags present in populated_relations");
        assert_eq!(populated_list.len(), 1);
        assert_eq!(populated_list[0].id, tag.id);
    }

    #[tokio::test]
    async fn test_update_with_relation_set_disconnect_and_unset() {
        let (service, article_type, tag_type, caller, title_attr, tags_attr, _) =
            make_relational_fixture();

        let tag1 = service
            .create(
                &caller,
                CreateDocumentCommand::new(
                    &tag_type.id,
                    HashMap::from([(
                        AttributeId::try_new("name").unwrap(),
                        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                            "Tag 1".into(),
                        ))),
                    )]),
                ),
            )
            .await
            .unwrap();

        let tag2 = service
            .create(
                &caller,
                CreateDocumentCommand::new(
                    &tag_type.id,
                    HashMap::from([(
                        AttributeId::try_new("name").unwrap(),
                        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                            "Tag 2".into(),
                        ))),
                    )]),
                ),
            )
            .await
            .unwrap();

        // Create article with no relations
        let article = service
            .create(
                &caller,
                CreateDocumentCommand::new(
                    &article_type.id,
                    HashMap::from([(
                        title_attr.clone(),
                        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                            "Post".into(),
                        ))),
                    )]),
                ),
            )
            .await
            .unwrap();

        // 1. Update with Set [tag1, tag2] and populate
        let update_cmd = UpdateDocumentCommand::new(article.id, &article_type.id, HashMap::new())
            .with_relations(HashMap::from([(
                tags_attr.clone(),
                RelationAction::Set(vec![tag1.id, tag2.id]),
            )]))
            .with_populate(vec![tags_attr.clone()]);

        let updated = service.update(&caller, update_cmd).await.unwrap();
        assert_eq!(
            updated.populated_relations.get(&tags_attr).unwrap().len(),
            2
        );

        // 2. Update with Disconnect [tag1]
        let disconnect_cmd =
            UpdateDocumentCommand::new(article.id, &article_type.id, HashMap::new())
                .with_relations(HashMap::from([(
                    tags_attr.clone(),
                    RelationAction::Disconnect(vec![tag1.id]),
                )]))
                .with_populate(vec![tags_attr.clone()]);

        let updated2 = service.update(&caller, disconnect_cmd).await.unwrap();
        let remaining = updated2.populated_relations.get(&tags_attr).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, tag2.id);

        // 3. Update with Unset
        let unset_cmd = UpdateDocumentCommand::new(article.id, &article_type.id, HashMap::new())
            .with_relations(HashMap::from([(tags_attr.clone(), RelationAction::Unset)]))
            .with_populate(vec![tags_attr.clone()]);

        let updated3 = service.update(&caller, unset_cmd).await.unwrap();
        assert!(!updated3.relations.contains_key(&tags_attr));
    }

    #[tokio::test]
    async fn test_has_one_cardinality_validation() {
        let (service, article_type, _, caller, title_attr, _, cat_attr) = make_relational_fixture();

        let cat1_id = DocumentInstanceId::new(Uuid::now_v7());
        let cat2_id = DocumentInstanceId::new(Uuid::now_v7());

        let cmd = CreateDocumentCommand::new(
            &article_type.id,
            HashMap::from([(
                title_attr,
                ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                    "Article".into(),
                ))),
            )]),
        )
        .with_relations(HashMap::from([(
            cat_attr,
            RelationAction::Set(vec![cat1_id, cat2_id]),
        )]));

        let res = service.create(&caller, cmd).await;
        assert!(
            matches!(res, Err(ApplicationError::Validation(msgs)) if msgs[0].contains("HasOne"))
        );
    }
}
