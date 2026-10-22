//! Commands for document instance operations.

use std::collections::HashMap;

pub use domain::content::{UnpublishDraftResolution, VersionView};
use domain::content::{ContentValue, DocumentInstanceId, FieldFilter, Pagination};
use domain::schema::{AttributeId, DocumentTypeId};

/// Query command to find documents matching criteria with pagination and optional relation enrichment.
#[derive(Debug, Clone, PartialEq)]
pub struct FindDocumentsCommand {
    /// Target document type to query.
    pub document_type: &'static DocumentTypeId,
    /// Pagination parameters (page number and page size).
    pub pagination: Pagination,
    /// Lifecycle representation to retrieve (`Published` by default for visitors, `Working` for admin drafts).
    pub view: VersionView,
    /// Field filters to apply.
    pub filters: Vec<FieldFilter>,
    /// Optional list of relation attribute IDs to populate in the returned documents.
    pub populate: Option<Vec<AttributeId>>,
}

impl FindDocumentsCommand {
    pub fn new(document_type: &'static DocumentTypeId, pagination: Pagination) -> Self {
        Self {
            document_type,
            pagination,
            view: VersionView::Working,
            filters: Vec::new(),
            populate: None,
        }
    }

    pub fn with_view(mut self, view: VersionView) -> Self {
        self.view = view;
        self
    }

    pub fn with_filters(mut self, filters: Vec<FieldFilter>) -> Self {
        self.filters = filters;
        self
    }

    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = Some(populate);
        self
    }
}

/// Query command to find a single document instance by identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindByIdCommand {
    /// Document type of the target document.
    pub document_type: &'static DocumentTypeId,
    /// Unique identifier of the document instance.
    pub document_instance_id: DocumentInstanceId,
    /// Lifecycle representation to retrieve (`Published` by default, or `Working` for the Admin draft tab).
    pub view: VersionView,
    /// Optional list of relation attribute IDs to populate in the returned document.
    pub populate: Option<Vec<AttributeId>>,
}

impl FindByIdCommand {
    pub fn new(
        document_type: &'static DocumentTypeId,
        document_instance_id: DocumentInstanceId,
    ) -> Self {
        Self {
            document_type,
            document_instance_id,
            view: VersionView::default(),
            populate: None,
        }
    }

    pub fn with_view(mut self, view: VersionView) -> Self {
        self.view = view;
        self
    }

    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = Some(populate);
        self
    }
}

/// Relational mutation actions for document creation and update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationAction {
    /// Replaces all existing relations for this attribute with the given target IDs.
    Set(Vec<DocumentInstanceId>),
    /// Appends the specified target IDs to existing relations without removing current links.
    Connect(Vec<DocumentInstanceId>),
    /// Removes the specified target IDs from existing relations.
    Disconnect(Vec<DocumentInstanceId>),
    /// Clears all relations for this attribute.
    Unset,
}

/// Command to create a new draft document instance.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateDocumentCommand {
    /// Target document type to create an instance for.
    pub document_type: &'static DocumentTypeId,
    /// Initial field values keyed by attribute ID.
    pub fields: HashMap<AttributeId, ContentValue>,
    /// Relational mutations keyed by attribute ID.
    pub relations: HashMap<AttributeId, RelationAction>,
    /// Optional list of relation attribute IDs to populate in the returned document.
    pub populate: Option<Vec<AttributeId>>,
}

impl CreateDocumentCommand {
    pub fn new(
        document_type: &'static DocumentTypeId,
        fields: HashMap<AttributeId, ContentValue>,
    ) -> Self {
        Self {
            document_type,
            fields,
            relations: HashMap::new(),
            populate: None,
        }
    }

    pub fn with_relations(mut self, relations: HashMap<AttributeId, RelationAction>) -> Self {
        self.relations = relations;
        self
    }

    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = Some(populate);
        self
    }
}

/// Command to update field values of an existing document instance.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateDocumentCommand {
    /// Identifier of the document instance to update.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: &'static DocumentTypeId,
    /// Updated field values keyed by attribute ID.
    pub fields: HashMap<AttributeId, ContentValue>,
    /// Relational mutations keyed by attribute ID.
    pub relations: HashMap<AttributeId, RelationAction>,
    /// Optional list of relation attribute IDs to populate in the returned document.
    pub populate: Option<Vec<AttributeId>>,
    /// Expected version for optimistic concurrency control (OCC).
    pub expected_version: Option<u32>,
}

impl UpdateDocumentCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: &'static DocumentTypeId,
        fields: HashMap<AttributeId, ContentValue>,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
            fields,
            relations: HashMap::new(),
            populate: None,
            expected_version: None,
        }
    }

    pub fn with_expected_version(mut self, version: u32) -> Self {
        self.expected_version = Some(version);
        self
    }

    pub fn with_relations(mut self, relations: HashMap<AttributeId, RelationAction>) -> Self {
        self.relations = relations;
        self
    }

    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = Some(populate);
        self
    }
}

/// Command to delete a document instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteDocumentCommand {
    /// Identifier of the document instance to delete.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: &'static DocumentTypeId,
}

impl DeleteDocumentCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: &'static DocumentTypeId,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
        }
    }
}

/// Command to publish a draft document instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishDocumentCommand {
    /// Identifier of the document instance to publish.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: &'static DocumentTypeId,
    /// Optional expected draft version for optimistic concurrency control.
    pub expected_version: Option<u32>,
}

impl PublishDocumentCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: &'static DocumentTypeId,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
            expected_version: None,
        }
    }

    pub fn with_expected_version(mut self, version: u32) -> Self {
        self.expected_version = Some(version);
        self
    }
}

/// Command to unpublish a published document instance back to draft status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpublishDocumentCommand {
    /// Identifier of the document instance to unpublish.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: &'static DocumentTypeId,
    /// Strategy for resolving pending working draft edits when unpublishing a modified document.
    pub draft_resolution: UnpublishDraftResolution,
}

impl UnpublishDocumentCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: &'static DocumentTypeId,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
            draft_resolution: UnpublishDraftResolution::default(),
        }
    }

    pub fn with_draft_resolution(mut self, resolution: UnpublishDraftResolution) -> Self {
        self.draft_resolution = resolution;
        self
    }
}

/// Command to discard pending draft edits and revert to the live published state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardDraftCommand {
    pub document_instance_id: DocumentInstanceId,
    pub document_type: &'static DocumentTypeId,
}

impl DiscardDraftCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: &'static DocumentTypeId,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
        }
    }
}

/// Command to restore a historical snapshot revision into the working draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreSnapshotCommand {
    pub document_instance_id: DocumentInstanceId,
    pub document_type: &'static DocumentTypeId,
    pub revision: u32,
    /// When true, allows restoring a snapshot even if the document currently has uncommitted working draft edits (`Modified` state).
    pub discard_pending_edits: bool,
}

impl RestoreSnapshotCommand {
    pub fn new(
        document_instance_id: DocumentInstanceId,
        document_type: &'static DocumentTypeId,
        revision: u32,
    ) -> Self {
        Self {
            document_instance_id,
            document_type,
            revision,
            discard_pending_edits: false,
        }
    }

    pub fn with_discard_pending_edits(mut self) -> Self {
        self.discard_pending_edits = true;
        self
    }
}

/// Query command to list lightweight document headers for the Admin UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAdminHeadersCommand {
    pub document_type: &'static DocumentTypeId,
    pub pagination: Pagination,
    pub status_filter: Option<domain::content::DocumentStatus>,
}

impl ListAdminHeadersCommand {
    pub fn new(document_type: &'static DocumentTypeId, pagination: Pagination) -> Self {
        Self {
            document_type,
            pagination,
            status_filter: None,
        }
    }

    pub fn with_status_filter(mut self, status: domain::content::DocumentStatus) -> Self {
        self.status_filter = Some(status);
        self
    }
}

/// Query command to list revision snapshot headers for a document instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSnapshotsCommand {
    pub document_type: &'static DocumentTypeId,
    pub document_instance_id: DocumentInstanceId,
}

impl ListSnapshotsCommand {
    pub fn new(
        document_type: &'static DocumentTypeId,
        document_instance_id: DocumentInstanceId,
    ) -> Self {
        Self {
            document_type,
            document_instance_id,
        }
    }
}

/// Query command to inspect a specific historical revision snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetSnapshotCommand {
    pub document_type: &'static DocumentTypeId,
    pub document_instance_id: DocumentInstanceId,
    pub revision: u32,
}

impl GetSnapshotCommand {
    pub fn new(
        document_type: &'static DocumentTypeId,
        document_instance_id: DocumentInstanceId,
        revision: u32,
    ) -> Self {
        Self {
            document_type,
            document_instance_id,
            revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::content::DocumentStatus;
    use uuid::Uuid;

    #[test]
    fn test_find_documents_command_builder() {
        let type_id: &'static DocumentTypeId =
            Box::leak(Box::new(DocumentTypeId::try_new("article").unwrap()));
        let attr = AttributeId::try_new("author").unwrap();
        let cmd = FindDocumentsCommand::new(type_id, Pagination::default())
            .with_populate(vec![attr.clone()]);

        assert_eq!(cmd.document_type, type_id);
        assert_eq!(cmd.pagination, Pagination::default());
        assert_eq!(cmd.view, VersionView::Working);
        assert_eq!(cmd.populate, Some(vec![attr]));
        assert!(cmd.filters.is_empty());

        let pub_cmd = FindDocumentsCommand::new(type_id, Pagination::default())
            .with_view(VersionView::Published);
        assert_eq!(pub_cmd.view, VersionView::Published);
    }

    #[test]
    fn test_find_by_id_command_builder() {
        let type_id: &'static DocumentTypeId =
            Box::leak(Box::new(DocumentTypeId::try_new("article").unwrap()));
        let id = DocumentInstanceId::new(Uuid::now_v7());
        let attr = AttributeId::try_new("category").unwrap();

        let cmd = FindByIdCommand::new(type_id, id).with_populate(vec![attr.clone()]);
        assert_eq!(cmd.document_type, type_id);
        assert_eq!(cmd.document_instance_id, id);
        assert_eq!(cmd.view, VersionView::Published);
        assert_eq!(cmd.populate, Some(vec![attr]));

        let working_cmd = FindByIdCommand::new(type_id, id).with_view(VersionView::Working);
        assert_eq!(working_cmd.view, VersionView::Working);
    }

    #[test]
    fn test_document_lifecycle_commands() {
        let type_id: &'static DocumentTypeId =
            Box::leak(Box::new(DocumentTypeId::try_new("article").unwrap()));
        let id = DocumentInstanceId::new(Uuid::now_v7());

        let create = CreateDocumentCommand::new(type_id, HashMap::new());
        assert_eq!(create.document_type, type_id);
        assert!(create.fields.is_empty());

        let update = UpdateDocumentCommand::new(id, type_id, HashMap::new())
            .with_expected_version(2);
        assert_eq!(update.document_instance_id, id);
        assert_eq!(update.document_type, type_id);
        assert_eq!(update.expected_version, Some(2));

        let delete = DeleteDocumentCommand::new(id, type_id);
        assert_eq!(delete.document_instance_id, id);
        assert_eq!(delete.document_type, type_id);

        let get_snap = GetSnapshotCommand::new(type_id, id, 1);
        assert_eq!(get_snap.document_type, type_id);
        assert_eq!(get_snap.document_instance_id, id);
        assert_eq!(get_snap.revision, 1);

        let pub_cmd = PublishDocumentCommand::new(id, type_id).with_expected_version(3);
        assert_eq!(pub_cmd.document_instance_id, id);
        assert_eq!(pub_cmd.expected_version, Some(3));

        let unpub_cmd = UnpublishDocumentCommand::new(id, type_id);
        assert_eq!(unpub_cmd.document_instance_id, id);
        assert_eq!(
            unpub_cmd.draft_resolution,
            UnpublishDraftResolution::KeepDraft
        );

        let unpub_replace = UnpublishDocumentCommand::new(id, type_id)
            .with_draft_resolution(UnpublishDraftResolution::ReplaceWithPublished);
        assert_eq!(
            unpub_replace.draft_resolution,
            UnpublishDraftResolution::ReplaceWithPublished
        );

        let discard = DiscardDraftCommand::new(id, type_id);
        assert_eq!(discard.document_instance_id, id);
        assert_eq!(discard.document_type, type_id);

        let restore = RestoreSnapshotCommand::new(id, type_id, 2);
        assert_eq!(restore.document_instance_id, id);
        assert_eq!(restore.document_type, type_id);
        assert_eq!(restore.revision, 2);

        let admin_cmd = ListAdminHeadersCommand::new(type_id, Pagination::default())
            .with_status_filter(DocumentStatus::Draft);
        assert_eq!(admin_cmd.document_type, type_id);
        assert_eq!(admin_cmd.status_filter, Some(DocumentStatus::Draft));

        let snapshots_cmd = ListSnapshotsCommand::new(type_id, id);
        assert_eq!(snapshots_cmd.document_type, type_id);
        assert_eq!(snapshots_cmd.document_instance_id, id);
    }
}
