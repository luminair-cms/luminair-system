//! Commands for document instance operations.

use std::collections::HashMap;

use domain::content::{ContentValue, DocumentInstanceId, FieldFilter, Pagination};
pub use domain::content::{UnpublishDraftResolution, VersionView};
use domain::schema::{AttributeId, DocumentTypeId};

// ---------------------------------------------------------------------------
// Command Types
// ---------------------------------------------------------------------------

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
    /// List of relation attribute IDs to populate in the returned documents.
    pub populate: Vec<AttributeId>,
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
    /// List of relation attribute IDs to populate in the returned document.
    pub populate: Vec<AttributeId>,
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
    /// List of relation attribute IDs to populate in the returned document.
    pub populate: Vec<AttributeId>,
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
    /// List of relation attribute IDs to populate in the returned document.
    pub populate: Vec<AttributeId>,
    /// Expected version for optimistic concurrency control (OCC).
    pub expected_version: Option<u32>,
}

/// Command to delete a document instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteDocumentCommand {
    /// Identifier of the document instance to delete.
    pub document_instance_id: DocumentInstanceId,
    /// Document type of the target document.
    pub document_type: &'static DocumentTypeId,
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

/// Command to discard pending draft edits and revert to the live published state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscardDraftCommand {
    pub document_instance_id: DocumentInstanceId,
    pub document_type: &'static DocumentTypeId,
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

/// Query command to list lightweight document headers for the Admin UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAdminHeadersCommand {
    pub document_type: &'static DocumentTypeId,
    pub pagination: Pagination,
    pub status_filter: Option<domain::content::DocumentStatus>,
}

/// Query command to list revision snapshot headers for a document instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSnapshotsCommand {
    pub document_type: &'static DocumentTypeId,
    pub document_instance_id: DocumentInstanceId,
}

/// Query command to inspect a specific historical revision snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetSnapshotCommand {
    pub document_type: &'static DocumentTypeId,
    pub document_instance_id: DocumentInstanceId,
    pub revision: u32,
}

// ---------------------------------------------------------------------------
// Command Implementations
// ---------------------------------------------------------------------------

impl FindDocumentsCommand {
    #[must_use]
    pub fn new(document_type: &'static DocumentTypeId, pagination: Pagination) -> Self {
        Self {
            document_type,
            pagination,
            view: VersionView::Working,
            filters: Vec::new(),
            populate: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_view(mut self, view: VersionView) -> Self {
        self.view = view;
        self
    }

    #[must_use]
    pub fn with_filters(mut self, filters: Vec<FieldFilter>) -> Self {
        self.filters = filters;
        self
    }

    #[must_use]
    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = populate;
        self
    }
}

impl FindByIdCommand {
    #[must_use]
    pub fn new(
        document_type: &'static DocumentTypeId,
        document_instance_id: DocumentInstanceId,
    ) -> Self {
        Self {
            document_type,
            document_instance_id,
            view: VersionView::default(),
            populate: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_view(mut self, view: VersionView) -> Self {
        self.view = view;
        self
    }

    #[must_use]
    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = populate;
        self
    }
}

impl CreateDocumentCommand {
    #[must_use]
    pub fn new(
        document_type: &'static DocumentTypeId,
        fields: HashMap<AttributeId, ContentValue>,
    ) -> Self {
        Self {
            document_type,
            fields,
            relations: HashMap::new(),
            populate: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_relations(mut self, relations: HashMap<AttributeId, RelationAction>) -> Self {
        self.relations = relations;
        self
    }

    #[must_use]
    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = populate;
        self
    }
}

impl UpdateDocumentCommand {
    #[must_use]
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
            populate: Vec::new(),
            expected_version: None,
        }
    }

    #[must_use]
    pub fn with_expected_version(mut self, version: u32) -> Self {
        self.expected_version = Some(version);
        self
    }

    #[must_use]
    pub fn with_relations(mut self, relations: HashMap<AttributeId, RelationAction>) -> Self {
        self.relations = relations;
        self
    }

    #[must_use]
    pub fn with_populate(mut self, populate: Vec<AttributeId>) -> Self {
        self.populate = populate;
        self
    }
}

impl DeleteDocumentCommand {
    #[must_use]
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

impl PublishDocumentCommand {
    #[must_use]
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

    #[must_use]
    pub fn with_expected_version(mut self, version: u32) -> Self {
        self.expected_version = Some(version);
        self
    }
}

impl UnpublishDocumentCommand {
    #[must_use]
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

    #[must_use]
    pub fn with_draft_resolution(mut self, resolution: UnpublishDraftResolution) -> Self {
        self.draft_resolution = resolution;
        self
    }
}

impl DiscardDraftCommand {
    #[must_use]
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

impl RestoreSnapshotCommand {
    #[must_use]
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

    #[must_use]
    pub fn with_discard_pending_edits(mut self) -> Self {
        self.discard_pending_edits = true;
        self
    }
}

impl ListAdminHeadersCommand {
    #[must_use]
    pub fn new(document_type: &'static DocumentTypeId, pagination: Pagination) -> Self {
        Self {
            document_type,
            pagination,
            status_filter: None,
        }
    }

    #[must_use]
    pub fn with_status_filter(mut self, status: domain::content::DocumentStatus) -> Self {
        self.status_filter = Some(status);
        self
    }
}

impl ListSnapshotsCommand {
    #[must_use]
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

impl GetSnapshotCommand {
    #[must_use]
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

    fn test_type() -> &'static DocumentTypeId {
        Box::leak(Box::new(DocumentTypeId::try_new("articles").unwrap()))
    }

    #[test]
    fn test_find_documents_command_builder() {
        let type_id = test_type();
        let pagination = Pagination {
            page: 1,
            page_size: 10,
        };
        let cmd = FindDocumentsCommand::new(type_id, pagination)
            .with_view(VersionView::Published)
            .with_filters(vec![])
            .with_populate(vec![]);

        assert_eq!(cmd.view, VersionView::Published);
        assert_eq!(cmd.pagination.page_size, 10);
    }

    #[test]
    fn test_find_by_id_command_builder() {
        let type_id = test_type();
        let id = DocumentInstanceId::new(Uuid::now_v7());
        let cmd = FindByIdCommand::new(type_id, id)
            .with_view(VersionView::Working)
            .with_populate(vec![]);

        assert_eq!(cmd.document_instance_id, id);
        assert_eq!(cmd.view, VersionView::Working);
    }

    #[test]
    fn test_document_lifecycle_commands() {
        let type_id = test_type();
        let id = DocumentInstanceId::new(Uuid::now_v7());

        let create = CreateDocumentCommand::new(type_id, HashMap::new())
            .with_relations(HashMap::new())
            .with_populate(vec![]);
        assert_eq!(create.document_type.as_ref(), "articles");

        let update = UpdateDocumentCommand::new(id, type_id, HashMap::new())
            .with_expected_version(3)
            .with_relations(HashMap::new())
            .with_populate(vec![]);
        assert_eq!(update.expected_version, Some(3));

        let delete = DeleteDocumentCommand::new(id, type_id);
        assert_eq!(delete.document_instance_id, id);

        let publish = PublishDocumentCommand::new(id, type_id).with_expected_version(2);
        assert_eq!(publish.expected_version, Some(2));

        let unpublish = UnpublishDocumentCommand::new(id, type_id)
            .with_draft_resolution(UnpublishDraftResolution::ReplaceWithPublished);
        assert_eq!(
            unpublish.draft_resolution,
            UnpublishDraftResolution::ReplaceWithPublished
        );

        let discard = DiscardDraftCommand::new(id, type_id);
        assert_eq!(discard.document_instance_id, id);

        let restore = RestoreSnapshotCommand::new(id, type_id, 5).with_discard_pending_edits();
        assert_eq!(restore.revision, 5);
        assert!(restore.discard_pending_edits);

        let pagination = Pagination {
            page: 1,
            page_size: 25,
        };
        let list_admin =
            ListAdminHeadersCommand::new(type_id, pagination).with_status_filter(DocumentStatus::Draft);
        assert_eq!(list_admin.status_filter, Some(DocumentStatus::Draft));

        let list_snaps = ListSnapshotsCommand::new(type_id, id);
        assert_eq!(list_snaps.document_instance_id, id);

        let get_snap = GetSnapshotCommand::new(type_id, id, 3);
        assert_eq!(get_snap.revision, 3);
    }
}
