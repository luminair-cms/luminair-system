//! Document instances application service.

use std::collections::{HashMap, HashSet};
use std::future::Future;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use domain::auth::Permission;
use domain::content::{
    extract_display_identity, validate_draft_content, validate_publish_content, DocumentHeader,
    DocumentInstance, DocumentInstanceId, DocumentInstanceRepository, DocumentSnapshot,
    DocumentSnapshotRepository, PublicationState, RelationMap, SnapshotHeader, SnapshotId,
    SnapshotReason, UnpublishDraftResolution, VersionView,
};
use domain::errors::DomainError;
use domain::schema::{
    AttributeId, DocumentKind, DocumentType, DocumentTypeId, RelationDefinition, RelationType,
};
use domain::system::SystemContext;

use crate::commands::documents::{
    CreateDocumentCommand, DeleteDocumentCommand, DiscardDraftCommand, FindByIdCommand,
    FindDocumentsCommand, GetSnapshotCommand, ListAdminHeadersCommand, ListSnapshotsCommand,
    PublishDocumentCommand, RelationAction, RestoreSnapshotCommand, UnpublishDocumentCommand,
    UpdateDocumentCommand,
};
use crate::context::CallerContext;
use crate::errors::ApplicationError;

/// Port trait defining use cases for document instances.
pub trait DocumentsService: Send + Sync + 'static {
    /// Queries a paginated list of documents with sequential count and optional relation enrichment.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if unauthorized or on repository query failure.
    fn find(
        &self,
        caller: &CallerContext,
        cmd: FindDocumentsCommand,
    ) -> impl Future<Output = Result<(Vec<DocumentInstance>, u64), ApplicationError>> + Send;

    /// Fetches a single document instance by identifier with optional relation enrichment.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if unauthorized or on repository query failure.
    fn find_by_id(
        &self,
        caller: &CallerContext,
        cmd: FindByIdCommand,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, ApplicationError>> + Send;

    /// Creates a new draft document instance, enforcing singleton limits and schema validation.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if validation fails, unauthorized, or duplicate singleton exists.
    fn create(
        &self,
        caller: &CallerContext,
        cmd: CreateDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Updates field values of an existing document instance.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] on optimistic concurrency failure, validation error, or unauthorized.
    fn update(
        &self,
        caller: &CallerContext,
        cmd: UpdateDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Deletes a document instance by identifier.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if unauthorized, not found, or on storage failure.
    fn delete(
        &self,
        caller: &CallerContext,
        cmd: DeleteDocumentCommand,
    ) -> impl Future<Output = Result<(), ApplicationError>> + Send;

    /// Publishes a draft document instance.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if publication validation fails, unauthorized, or on conflict.
    fn publish(
        &self,
        caller: &CallerContext,
        cmd: PublishDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Reverts a published document instance back to draft state (not visible for public use).
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if unauthorized or state transition fails.
    fn unpublish(
        &self,
        caller: &CallerContext,
        cmd: UnpublishDocumentCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Discards pending draft edits, reverting the document to its clean live published state.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if the document is not in Modified state or unauthorized.
    fn discard_draft(
        &self,
        caller: &CallerContext,
        cmd: DiscardDraftCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Restores a historical snapshot revision into the current draft.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if snapshot not found, conflict with pending edits, or unauthorized.
    fn restore_snapshot(
        &self,
        caller: &CallerContext,
        cmd: RestoreSnapshotCommand,
    ) -> impl Future<Output = Result<DocumentInstance, ApplicationError>> + Send;

    /// Lists lightweight document headers for Admin Studio views with 2-phase relational enrichment.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if unauthorized or on storage query error.
    fn list_admin_headers(
        &self,
        caller: &CallerContext,
        cmd: ListAdminHeadersCommand,
    ) -> impl Future<Output = Result<(Vec<DocumentHeader>, u64), ApplicationError>> + Send;

    /// Lists revision snapshot headers for a document instance.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if unauthorized or on storage query error.
    fn list_snapshots(
        &self,
        caller: &CallerContext,
        cmd: ListSnapshotsCommand,
    ) -> impl Future<Output = Result<Vec<SnapshotHeader>, ApplicationError>> + Send;

    /// Fetches a specific historical revision snapshot by revision number.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if snapshot is not found or unauthorized.
    fn get_snapshot(
        &self,
        caller: &CallerContext,
        cmd: GetSnapshotCommand,
    ) -> impl Future<Output = Result<DocumentSnapshot, ApplicationError>> + Send;
}

/// Generic implementation of `DocumentsService` monomorphized over repository adapters.
pub struct DocumentsServiceImpl<R, S = R> {
    pub instance_repo: R,
    pub snapshot_repo: S,
    pub context: &'static SystemContext,
}

impl<Repo> DocumentsServiceImpl<Repo, Repo>
where
    Repo: DocumentInstanceRepository + DocumentSnapshotRepository + Clone + 'static,
{
    pub fn new(instance_repo: Repo, context: &'static SystemContext) -> Self {
        let snapshot_repo = instance_repo.clone();
        Self {
            instance_repo,
            snapshot_repo,
            context,
        }
    }
}

impl<R, S> DocumentsServiceImpl<R, S>
where
    R: DocumentInstanceRepository + 'static,
    S: DocumentSnapshotRepository + 'static,
{
    pub fn new_with_snapshots(
        instance_repo: R,
        snapshot_repo: S,
        context: &'static SystemContext,
    ) -> Self {
        Self {
            instance_repo,
            snapshot_repo,
            context,
        }
    }

    /// Two-phase batch relation enrichment:
    /// 1. Fetches related instances for all parent instances in a single batch query.
    /// 2. Stitches relations in-memory to each corresponding document instance.
    ///
    /// # Errors
    /// Returns [`ApplicationError`] if the repository fails to fetch relations.
    pub async fn enrich(
        &self,
        type_id: &DocumentTypeId,
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
    ///
    /// # Errors
    /// Returns [`ApplicationError::Validation`] if a relation attribute is invalid or violates cardinality.
    pub fn validate_and_apply_relations(
        &self,
        doc_type: &DocumentType,
        instance: &mut DocumentInstance,
        relations: HashMap<AttributeId, RelationAction>,
    ) -> Result<(), ApplicationError> {
        for (attr, action) in relations {
            let rel = validate_owning_relation(doc_type, &attr)?;
            let is_has_one = rel.relation_type == RelationType::HasOne;
            apply_relation_action(instance, attr, action, is_has_one)?;
        }
        Ok(())
    }
}

impl<R, S> DocumentsService for DocumentsServiceImpl<R, S>
where
    R: DocumentInstanceRepository + 'static,
    S: DocumentSnapshotRepository + 'static,
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

        // Sequential fetch for MVP (routes to published wide table for visitors, or drafts store for working copies)
        let page = match cmd.view {
            VersionView::Published => {
                self.instance_repo
                    .find_published(cmd.document_type, cmd.pagination, cmd.filters.clone())
                    .await?
            }
            VersionView::Working => {
                self.instance_repo
                    .find_by_type(cmd.document_type, cmd.pagination, cmd.filters.clone())
                    .await?
            }
        };
        let count = page.total;

        // Two-phase batch relation enrichment
        let enriched = self
            .enrich(cmd.document_type, &cmd.populate, page.items)
            .await?;

        Ok((enriched, count))
    }

    async fn find_by_id(
        &self,
        caller: &CallerContext,
        cmd: FindByIdCommand,
    ) -> Result<Option<DocumentInstance>, ApplicationError> {
        let Some(instance) = self
            .instance_repo
            .find_by_id(cmd.document_type, cmd.document_instance_id, cmd.view)
            .await?
        else {
            return Ok(None);
        };

        caller.check_permission(
            &Permission::ReadDocument(Some(instance.document_type_id.clone())),
            Some(&instance),
        )?;

        let enriched = self
            .enrich_single(cmd.document_type, &cmd.populate, instance)
            .await?;
        Ok(Some(enriched))
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

        self.check_singleton_guard(doc_type).await?;

        let mut instance =
            DocumentInstance::new(cmd.document_type.clone(), caller.actor.clone(), Utc::now());
        instance.replace_fields(cmd.fields);

        self.validate_content_for_type(doc_type, &instance)?;
        self.validate_and_apply_relations(doc_type, &mut instance, cmd.relations)?;

        self.persist_new_instance(doc_type, &mut instance, &caller.actor)
            .await?;

        self.enrich_single(cmd.document_type, &cmd.populate, instance)
            .await
    }

    async fn update(
        &self,
        caller: &CallerContext,
        cmd: UpdateDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let (doc_type, mut instance) = self
            .load_working_instance_with_permission(
                caller,
                cmd.document_type,
                cmd.document_instance_id,
                |tid| Permission::UpdateDocument(Some(tid)),
            )
            .await?;

        if let Some(expected) = cmd.expected_version
            && instance.audit.version != expected
        {
            return Err(ApplicationError::version_mismatch_conflict(
                expected,
                instance.audit.version,
            ));
        }

        let prev_published_snapshot = capture_superseded_single_lifecycle_snapshot(
            doc_type,
            &instance,
            &caller.actor,
        );
        apply_field_updates(&mut instance, cmd.fields);
        self.validate_content_for_type(doc_type, &instance)?;
        self.validate_and_apply_relations(doc_type, &mut instance, cmd.relations)?;

        self.persist_updated_instance(
            doc_type,
            &mut instance,
            &caller.actor,
            prev_published_snapshot.as_ref(),
        )
        .await?;

        self.enrich_single(cmd.document_type, &cmd.populate, instance)
            .await
    }

    async fn delete(
        &self,
        caller: &CallerContext,
        cmd: DeleteDocumentCommand,
    ) -> Result<(), ApplicationError> {
        let (_doc_type, _instance) = self
            .load_working_instance_with_permission(
                caller,
                cmd.document_type,
                cmd.document_instance_id,
                |tid| Permission::DeleteDocument(Some(tid)),
            )
            .await?;

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
        let (doc_type, mut instance) = self
            .load_working_instance_with_permission(
                caller,
                cmd.document_type,
                cmd.document_instance_id,
                |tid| Permission::PublishDocument(Some(tid)),
            )
            .await?;

        validate_publish_eligibility(doc_type, &instance, cmd.expected_version)?;
        self.validate_content_for_publish(doc_type, &instance)?;

        let now = Utc::now();
        let prev_published = self
            .instance_repo
            .find_by_id(
                cmd.document_type,
                cmd.document_instance_id,
                VersionView::Published,
            )
            .await?;

        let archived_snapshot = capture_superseded_live_snapshot(
            prev_published.as_ref(),
            caller.actor.clone(),
            now,
        );

        let rev = instance.publish(caller.actor.clone(), now)?;
        let (display_values, display_title) = extract_display_identity(
            doc_type,
            instance.id,
            instance.fields(),
            &self.context.config.default_locale,
        );

        self.instance_repo
            .publish(
                &instance,
                rev,
                &display_values,
                &display_title,
                archived_snapshot.as_ref(),
            )
            .await?;

        Ok(instance)
    }

    async fn unpublish(
        &self,
        caller: &CallerContext,
        cmd: UnpublishDocumentCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let (doc_type, mut instance) = self
            .load_working_instance_with_permission(
                caller,
                cmd.document_type,
                cmd.document_instance_id,
                |tid| Permission::PublishDocument(Some(tid)),
            )
            .await?;

        validate_unpublish_eligibility(doc_type, &instance)?;

        let live_published = self
            .instance_repo
            .find_by_id(
                cmd.document_type,
                cmd.document_instance_id,
                VersionView::Published,
            )
            .await?;

        let now = Utc::now();
        let snapshot_source = live_published.as_ref().unwrap_or(&instance);
        let snapshot = build_unpublish_snapshot(snapshot_source, caller.actor.clone(), now);

        if cmd.draft_resolution == UnpublishDraftResolution::ReplaceWithPublished
            && let Some(ref live_inst) = live_published
        {
            instance.replace_fields(live_inst.fields().clone());
            instance.relations = live_inst.relations.clone();
        }

        instance.unpublish(caller.actor.clone(), now)?;

        let (display_values, display_title) = extract_display_identity(
            doc_type,
            instance.id,
            instance.fields(),
            &self.context.config.default_locale,
        );

        self.instance_repo
            .unpublish(&instance, &display_values, &display_title, &snapshot)
            .await?;

        Ok(instance)
    }

    async fn discard_draft(
        &self,
        caller: &CallerContext,
        cmd: DiscardDraftCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let (_doc_type, mut instance) = self
            .load_working_instance_with_permission(
                caller,
                cmd.document_type,
                cmd.document_instance_id,
                |tid| Permission::UpdateDocument(Some(tid)),
            )
            .await?;

        instance.discard_draft()?;

        let restored = self
            .instance_repo
            .discard_draft(cmd.document_type, cmd.document_instance_id)
            .await?;

        Ok(restored.unwrap_or(instance))
    }

    async fn restore_snapshot(
        &self,
        caller: &CallerContext,
        cmd: RestoreSnapshotCommand,
    ) -> Result<DocumentInstance, ApplicationError> {
        let (doc_type, mut instance) = self
            .load_working_instance_with_permission(
                caller,
                cmd.document_type,
                cmd.document_instance_id,
                |tid| Permission::UpdateDocument(Some(tid)),
            )
            .await?;

        // Draft guard: fresh drafts that have never been published cannot restore snapshots (LC-13)
        if matches!(instance.content.publication_state, PublicationState::Draft) {
            return Err(ApplicationError::Domain(
                DomainError::InvalidStateTransition {
                    reason: "cannot restore snapshot on a document in Draft state that has never been published".to_string(),
                }
            ));
        }

        let snapshot = self
            .snapshot_repo
            .get_snapshot(cmd.document_type, cmd.document_instance_id, cmd.revision)
            .await?
            .ok_or_else(|| ApplicationError::NotFound {
                entity: "DocumentSnapshot",
                id: format!("{}:v{}", cmd.document_instance_id, cmd.revision),
            })?;

        let now = Utc::now();
        check_restore_conflict(&instance, &cmd)?;
        instance.restore_snapshot(&snapshot, caller.actor.clone(), now)?;

        self.persist_restored_instance(doc_type, &mut instance, &caller.actor, now)
            .await?;

        Ok(instance)
    }

    async fn list_admin_headers(
        &self,
        caller: &CallerContext,
        cmd: ListAdminHeadersCommand,
    ) -> Result<(Vec<DocumentHeader>, u64), ApplicationError> {
        caller.check_permission(
            &Permission::ReadDocument(Some(cmd.document_type.clone())),
            None,
        )?;

        let doc_type = self.context.find_type(cmd.document_type).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(cmd.document_type.clone()))
        })?;

        let mut page = self
            .instance_repo
            .list_admin_headers(cmd.document_type, cmd.pagination, cmd.status_filter)
            .await?;

        self.enrich_admin_headers_relational_display(doc_type, &mut page.items)
            .await?;

        Ok((page.items, page.total))
    }

    async fn list_snapshots(
        &self,
        caller: &CallerContext,
        cmd: ListSnapshotsCommand,
    ) -> Result<Vec<SnapshotHeader>, ApplicationError> {
        caller.check_permission(
            &Permission::ReadDocument(Some(cmd.document_type.clone())),
            None,
        )?;

        self.context.find_type(cmd.document_type).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(cmd.document_type.clone()))
        })?;

        let snapshots = self
            .snapshot_repo
            .list_snapshots(cmd.document_type, cmd.document_instance_id)
            .await?;

        Ok(snapshots)
    }

    async fn get_snapshot(
        &self,
        caller: &CallerContext,
        cmd: GetSnapshotCommand,
    ) -> Result<DocumentSnapshot, ApplicationError> {
        caller.check_permission(
            &Permission::ReadDocument(Some(cmd.document_type.clone())),
            None,
        )?;

        self.context.find_type(cmd.document_type).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(cmd.document_type.clone()))
        })?;

        self.snapshot_repo
            .get_snapshot(cmd.document_type, cmd.document_instance_id, cmd.revision)
            .await?
            .ok_or_else(|| ApplicationError::NotFound {
                entity: "DocumentSnapshot",
                id: format!("{}:v{}", cmd.document_instance_id, cmd.revision),
            })
    }
}

impl<R, S> DocumentsServiceImpl<R, S>
where
    R: DocumentInstanceRepository + 'static,
    S: DocumentSnapshotRepository + 'static,
{
    async fn load_working_instance_with_permission(
        &self,
        caller: &CallerContext,
        type_id: &DocumentTypeId,
        instance_id: DocumentInstanceId,
        permission: impl FnOnce(DocumentTypeId) -> Permission,
    ) -> Result<(&'static DocumentType, DocumentInstance), ApplicationError> {
        let instance = self
            .instance_repo
            .find_by_id(type_id, instance_id, VersionView::Working)
            .await?
            .ok_or(ApplicationError::Domain(
                DomainError::DocumentInstanceNotFound(instance_id),
            ))?;

        caller.check_permission(
            &permission(instance.document_type_id.clone()),
            Some(&instance),
        )?;

        let doc_type = self.context.find_type(type_id).ok_or_else(|| {
            ApplicationError::Domain(DomainError::DocumentTypeNotFound(type_id.clone()))
        })?;

        Ok((doc_type, instance))
    }

    async fn check_singleton_guard(&self, doc_type: &DocumentType) -> Result<(), ApplicationError> {
        if doc_type.kind == DocumentKind::Singleton
            && self.instance_repo.exists_for_type(&doc_type.id).await?
        {
            return Err(ApplicationError::Domain(
                DomainError::SingletonAlreadyExists(doc_type.id.clone()),
            ));
        }
        Ok(())
    }

    async fn persist_new_instance(
        &self,
        doc_type: &DocumentType,
        instance: &mut DocumentInstance,
        actor: &domain::auth::ActorId,
    ) -> Result<(), ApplicationError> {
        let (display_values, display_title) = extract_display_identity(
            doc_type,
            instance.id,
            instance.fields(),
            &self.context.config.default_locale,
        );

        if doc_type.options.draft_and_publish {
            self.instance_repo
                .save_draft(instance, &display_values, &display_title)
                .await?;
        } else {
            let now = Utc::now();
            let rev = instance.publish(actor.clone(), now)?;
            self.instance_repo
                .publish(instance, rev, &display_values, &display_title, None)
                .await?;
        }
        Ok(())
    }

    async fn persist_updated_instance(
        &self,
        doc_type: &DocumentType,
        instance: &mut DocumentInstance,
        actor: &domain::auth::ActorId,
        prev_published_snapshot: Option<&DocumentSnapshot>,
    ) -> Result<(), ApplicationError> {
        let (display_values, display_title) = extract_display_identity(
            doc_type,
            instance.id,
            instance.fields(),
            &self.context.config.default_locale,
        );

        if doc_type.options.draft_and_publish {
            instance.edit_draft(actor.clone(), Utc::now());
            self.instance_repo
                .save_draft(instance, &display_values, &display_title)
                .await?;
        } else {
            let now = Utc::now();
            let rev = instance.publish(actor.clone(), now)?;
            self.instance_repo
                .publish(
                    instance,
                    rev,
                    &display_values,
                    &display_title,
                    prev_published_snapshot,
                )
                .await?;
        }
        Ok(())
    }

    async fn persist_restored_instance(
        &self,
        doc_type: &DocumentType,
        instance: &mut DocumentInstance,
        actor: &domain::auth::ActorId,
        now: DateTime<Utc>,
    ) -> Result<(), ApplicationError> {
        let (display_values, display_title) = extract_display_identity(
            doc_type,
            instance.id,
            instance.fields(),
            &self.context.config.default_locale,
        );

        if doc_type.options.draft_and_publish {
            self.instance_repo
                .save_draft(instance, &display_values, &display_title)
                .await?;
        } else {
            let prev_published = self
                .instance_repo
                .find_by_id(&doc_type.id, instance.id, VersionView::Published)
                .await?;
            let prev_snapshot = capture_superseded_live_snapshot(
                prev_published.as_ref(),
                actor.clone(),
                now,
            );
            let rev = instance.publish(actor.clone(), now)?;
            self.instance_repo
                .publish(
                    instance,
                    rev,
                    &display_values,
                    &display_title,
                    prev_snapshot.as_ref(),
                )
                .await?;
        }
        Ok(())
    }

    async fn enrich_single(
        &self,
        type_id: &DocumentTypeId,
        populate: &[AttributeId],
        instance: DocumentInstance,
    ) -> Result<DocumentInstance, ApplicationError> {
        if populate.is_empty() {
            return Ok(instance);
        }
        let mut enriched = self.enrich(type_id, populate, vec![instance]).await?;
        enriched.pop().ok_or_else(|| {
            ApplicationError::Internal("enriched instance missing from batch result".into())
        })
    }

    async fn enrich_admin_headers_relational_display(
        &self,
        doc_type: &DocumentType,
        items: &mut [DocumentHeader],
    ) -> Result<(), ApplicationError> {
        let Some(rel_def) = doc_type.relational_display_field() else {
            return Ok(());
        };

        let target_ids: Vec<DocumentInstanceId> = items
            .iter()
            .filter_map(|header| {
                header
                    .relational_display_target
                    .as_ref()
                    .filter(|(attr, _)| attr == &rel_def.id)
                    .map(|(_, target_id)| *target_id)
            })
            .collect();

        if target_ids.is_empty() {
            return Ok(());
        }

        let titles = self
            .instance_repo
            .find_display_titles_by_ids(&rel_def.target, &target_ids)
            .await?;

        let effective_fields = doc_type.effective_display_fields();

        for header in items {
            if let Some((attr, target_id)) = &header.relational_display_target
                && attr == &rel_def.id
                && let Some(target_title) = titles.get(target_id)
            {
                header.enrich_relational_display(&effective_fields, &rel_def.id, target_title);
            }
        }

        Ok(())
    }

    fn validate_content_for_type(
        &self,
        doc_type: &DocumentType,
        instance: &DocumentInstance,
    ) -> Result<(), ApplicationError> {
        let validation_res = if doc_type.options.draft_and_publish {
            validate_draft_content(doc_type, &instance.content, &self.context.config)
        } else {
            validate_publish_content(doc_type, &instance.content, &self.context.config)
        };

        validation_res.map_err(|errs| {
            let params = errs
                .iter()
                .map(crate::errors::InvalidParam::from_domain_error)
                .collect();
            ApplicationError::Validation(params)
        })
    }

    fn validate_content_for_publish(
        &self,
        doc_type: &DocumentType,
        instance: &DocumentInstance,
    ) -> Result<(), ApplicationError> {
        validate_publish_content(doc_type, &instance.content, &self.context.config).map_err(
            |errs| {
                let params = errs
                    .iter()
                    .map(crate::errors::InvalidParam::from_domain_error)
                    .collect();
                ApplicationError::Validation(params)
            },
        )
    }
}

fn validate_owning_relation<'a>(
    doc_type: &'a DocumentType,
    attr: &AttributeId,
) -> Result<&'a RelationDefinition, ApplicationError> {
    let rel = doc_type.relations.get(attr).ok_or_else(|| {
        ApplicationError::validation_single(
            attr.to_string(),
            format!(
                "relation attribute '{attr}' is not defined for document type '{}'",
                doc_type.id
            ),
            "unknown_attribute",
        )
    })?;

    if !rel.relation_type.is_owning() {
        return Err(ApplicationError::validation_single(
            attr.to_string(),
            format!(
                "relation attribute '{attr}' is an inverse-side relation; mutate from owner type '{:?}'",
                rel.relation_type
            ),
            "invalid_relation",
        ));
    }

    Ok(rel)
}

fn apply_relation_action(
    instance: &mut DocumentInstance,
    attr: AttributeId,
    action: RelationAction,
    is_has_one: bool,
) -> Result<(), ApplicationError> {
    match action {
        RelationAction::Set(target_ids) => {
            apply_set_relation(instance, attr, target_ids, is_has_one)
        }
        RelationAction::Connect(target_ids) => {
            apply_connect_relation(instance, &attr, target_ids, is_has_one)
        }
        RelationAction::Disconnect(target_ids) => {
            instance.disconnect_relations(&attr, &target_ids);
            Ok(())
        }
        RelationAction::Unset => {
            instance.unset_relations(&attr);
            Ok(())
        }
    }
}

fn apply_set_relation(
    instance: &mut DocumentInstance,
    attr: AttributeId,
    target_ids: Vec<DocumentInstanceId>,
    is_has_one: bool,
) -> Result<(), ApplicationError> {
    let unique: HashSet<_> = target_ids.iter().collect();
    if is_has_one && unique.len() > 1 {
        return Err(ApplicationError::validation_single(
            attr.to_string(),
            format!(
                "relation '{attr}' is HasOne but received {} target IDs",
                unique.len()
            ),
            "cardinality_violation",
        ));
    }
    instance.set_relations(attr, target_ids);
    Ok(())
}

fn apply_connect_relation(
    instance: &mut DocumentInstance,
    attr: &AttributeId,
    target_ids: Vec<DocumentInstanceId>,
    is_has_one: bool,
) -> Result<(), ApplicationError> {
    if is_has_one {
        let mut unique: HashSet<_> = target_ids.iter().collect();
        if let Some(existing) = instance.relations.get(attr) {
            for r in existing {
                unique.insert(&r.target_instance_id);
            }
        }
        if unique.len() > 1 {
            return Err(ApplicationError::validation_single(
                attr.to_string(),
                format!("relation '{attr}' is HasOne and cannot have multiple targets connected"),
                "cardinality_violation",
            ));
        }
    }
    instance.connect_relations(attr, target_ids);
    Ok(())
}

fn apply_field_updates(
    instance: &mut DocumentInstance,
    fields: HashMap<AttributeId, domain::content::ContentValue>,
) {
    for (attr, val) in fields {
        instance.set_field(attr, val);
    }
}

fn validate_publish_eligibility(
    doc_type: &DocumentType,
    instance: &DocumentInstance,
    expected_version: Option<u32>,
) -> Result<(), ApplicationError> {
    if !doc_type.options.draft_and_publish {
        return Err(ApplicationError::draft_publish_disabled_conflict(
            &doc_type.info.title,
        ));
    }

    if matches!(
        instance.content.publication_state,
        PublicationState::Published { .. }
    ) {
        return Err(ApplicationError::already_published_conflict(instance.id));
    }

    if let Some(expected) = expected_version
        && instance.audit.version != expected
    {
        return Err(ApplicationError::version_mismatch_conflict(
            expected,
            instance.audit.version,
        ));
    }

    Ok(())
}

fn validate_unpublish_eligibility(
    doc_type: &DocumentType,
    instance: &DocumentInstance,
) -> Result<(), ApplicationError> {
    if !doc_type.options.draft_and_publish {
        return Err(ApplicationError::draft_publish_disabled_conflict(
            &doc_type.info.title,
        ));
    }

    if matches!(
        instance.content.publication_state,
        PublicationState::Draft | PublicationState::Unpublished { .. }
    ) {
        return Err(ApplicationError::Domain(
            DomainError::InvalidStateTransition {
                reason: "cannot unpublish a document that is not currently published"
                    .to_string(),
            },
        ));
    }

    Ok(())
}

fn capture_superseded_single_lifecycle_snapshot(
    doc_type: &DocumentType,
    instance: &DocumentInstance,
    actor: &domain::auth::ActorId,
) -> Option<DocumentSnapshot> {
    if doc_type.options.draft_and_publish {
        return None;
    }
    capture_superseded_live_snapshot(Some(instance), actor.clone(), Utc::now())
}

fn check_restore_conflict(
    instance: &DocumentInstance,
    cmd: &RestoreSnapshotCommand,
) -> Result<(), ApplicationError> {
    if matches!(
        instance.content.publication_state,
        PublicationState::Modified { .. }
    ) && !cmd.discard_pending_edits
    {
        let draft_version = match instance.content.publication_state {
            PublicationState::Modified { draft_version, .. } => draft_version,
            _ => instance.audit.version,
        };
        return Err(ApplicationError::draft_overwrite_conflict(
            instance.id,
            cmd.revision,
            draft_version,
        ));
    }
    Ok(())
}

fn capture_superseded_live_snapshot(
    prev_live: Option<&DocumentInstance>,
    actor: domain::auth::ActorId,
    now: DateTime<Utc>,
) -> Option<DocumentSnapshot> {
    let live = prev_live?;
    let rev = match live.content.publication_state {
        PublicationState::Published { revision, .. } => revision,
        _ => 0,
    };
    if rev > 0 {
        Some(document_snapshot_from_instance(
            live,
            rev,
            actor,
            SnapshotReason::Publish,
            now,
        ))
    } else {
        None
    }
}

fn build_unpublish_snapshot(
    instance: &DocumentInstance,
    actor: domain::auth::ActorId,
    now: DateTime<Utc>,
) -> DocumentSnapshot {
    let rev = match instance.content.publication_state {
        PublicationState::Published { revision, .. } => revision,
        PublicationState::Modified { live_revision, .. } => live_revision,
        _ => 0,
    };
    document_snapshot_from_instance(instance, rev, actor, SnapshotReason::Unpublish, now)
}

fn document_snapshot_from_instance(
    instance: &DocumentInstance,
    revision: u32,
    actor: domain::auth::ActorId,
    reason: SnapshotReason,
    now: DateTime<Utc>,
) -> DocumentSnapshot {
    DocumentSnapshot {
        id: SnapshotId::new(Uuid::now_v7()),
        document_type_id: instance.document_type_id.clone(),
        document_id: instance.id,
        revision,
        fields: instance.fields().clone(),
        relations: instance.relations.clone(),
        created_at: now,
        created_by: actor,
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::auth::{ActorId, UserId};
    use domain::content::{
        ContentValue, DomainValue, Pagination, PrimitiveValue, PublicationState,
    };
    use domain::schema::{DocumentType, RelationDefinition, RelationType, SchemaRegistry};
    use domain::system::SystemContext;
    use domain::test_support::{DocumentTypeBuilder, fixture_system_config, text_field};
    use uuid::Uuid;

    use crate::errors::ConflictCode;
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
            DocumentKind::Singleton => DocumentTypeBuilder::singleton("article"),
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

        // Newly created draft is not published yet: default VersionView::Published returns None
        let published_res = service
            .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, created.id))
            .await
            .unwrap();
        assert!(published_res.is_none());

        // Working view returns the draft document
        let fetched = service
            .find_by_id(
                &caller,
                FindByIdCommand::new(&doc_type.id, created.id).with_view(VersionView::Working),
            )
            .await
            .unwrap()
            .expect("found document");

        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.document_type_id, doc_type.id);
    }

    #[tokio::test]
    async fn test_create_single_type_guard_blocks_duplicate() {
        let (service, doc_type, caller, title_attr) = make_test_fixture(DocumentKind::Singleton);

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

        // Second creation must fail with SingletonAlreadyExists
        let second = service
            .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
            .await;

        assert!(matches!(
            second,
            Err(ApplicationError::Domain(DomainError::SingletonAlreadyExists(id))) if id == doc_type.id
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
        let related_tag = DocumentInstance::new(tag_type_id, ActorId::System, Utc::now());

        service.instance_repo.add_relation_data(
            rel_attr.clone(),
            inst.id,
            vec![related_tag.clone()],
        );

        // Query with populate
        let cmd = FindDocumentsCommand::new(&doc_type.id, Pagination::default())
            .with_view(VersionView::Working)
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
            PublicationState::Unpublished {
                last_published_revision: 1,
                ..
            }
        ));
        assert_eq!(unpublished.audit.updated_by, caller.actor);
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
            matches!(
                publish_result,
                Err(ApplicationError::Conflict {
                    code: ConflictCode::DraftAndPublishDisabled,
                    ..
                })
            ),
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

        // After delete: instance is gone from both published and working views
        let found = service
            .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, created.id))
            .await
            .expect("find_by_id returns Ok");
        assert!(found.is_none(), "instance should be gone after delete");

        let found_working = service
            .find_by_id(
                &caller,
                FindByIdCommand::new(&doc_type.id, created.id).with_view(VersionView::Working),
            )
            .await
            .expect("find_by_id returns Ok");
        assert!(
            found_working.is_none(),
            "working instance should be gone after delete"
        );
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
