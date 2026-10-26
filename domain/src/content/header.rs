use std::collections::HashMap;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use nutype::nutype;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::ActorId;
use crate::content::instance::{DocumentInstanceId, PublicationState, ResolvedRelation};
use crate::content::values::{ContentValue, format_display_title};
use crate::schema::{AttributeId, DocumentTypeId};

/// Specifies which lifecycle representation of a single document instance to retrieve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionView {
    /// Strictly the live published representation from the wide table (`[type]`).
    /// This is the default for public visitor queries and the read-only "Published" tab in the Admin UI.
    #[default]
    Published,
    /// The latest working representation (`document_drafts` if present, otherwise falling back to `[type]`).
    /// Used for the editable "Draft" tab in the Admin UI.
    Working,
}

/// Specifies how pending draft changes should be handled when unpublishing a `Modified` document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnpublishDraftResolution {
    /// Keep the in-progress draft content in `document_drafts` (default).
    #[default]
    KeepDraft,
    /// Replace the draft content in `document_drafts` with the previously published live content.
    ReplaceWithPublished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Draft,
    Published,
    Modified,
    Unpublished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentHeader {
    pub id: DocumentInstanceId,
    pub document_type_id: DocumentTypeId,
    pub display_values: IndexMap<AttributeId, String>,
    pub display_title: String,
    pub relational_display_target: Option<(AttributeId, DocumentInstanceId)>,
    pub status: DocumentStatus,
    pub slug: Option<String>,
    pub revision: Option<u32>,
    pub draft_version: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub created_by: ActorId,
    pub updated_at: DateTime<Utc>,
    pub updated_by: ActorId,
    pub published_at: Option<DateTime<Utc>>,
    pub published_by: Option<ActorId>,
}

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize,
    AsRef,
    Deref,
    Into
))]
pub struct SnapshotId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotReason {
    Publish,
    Unpublish,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSnapshot {
    pub id: SnapshotId,
    pub document_type_id: DocumentTypeId,
    pub document_id: DocumentInstanceId,
    pub revision: u32,
    pub fields: HashMap<AttributeId, ContentValue>,
    pub relations: HashMap<AttributeId, Vec<ResolvedRelation>>,
    pub created_at: DateTime<Utc>,
    pub created_by: ActorId,
    pub reason: SnapshotReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotHeader {
    pub id: SnapshotId,
    pub document_type_id: DocumentTypeId,
    pub document_id: DocumentInstanceId,
    pub revision: u32,
    pub created_at: DateTime<Utc>,
    pub created_by: ActorId,
    pub reason: SnapshotReason,
}

impl DocumentHeader {
    /// Enriches this header's `display_values` and `display_title` with a resolved relational target title
    /// at the exact schema-defined attribute position in `effective_fields`.
    pub fn enrich_relational_display(
        &mut self,
        effective_fields: &[AttributeId],
        rel_attr: &AttributeId,
        target_title: &str,
    ) {
        let mut rebuilt = IndexMap::with_capacity(effective_fields.len());
        for field_id in effective_fields {
            if field_id == rel_attr {
                rebuilt.insert(field_id.clone(), target_title.to_string());
            } else if let Some(val) = self.display_values.swap_remove(field_id) {
                rebuilt.insert(field_id.clone(), val);
            }
        }
        self.display_values = rebuilt;
        self.display_title = format_display_title(&self.display_values, self.id);
    }
}

impl From<&PublicationState> for DocumentStatus {
    fn from(state: &PublicationState) -> Self {
        match state {
            PublicationState::Draft => DocumentStatus::Draft,
            PublicationState::Published { .. } => DocumentStatus::Published,
            PublicationState::Modified { .. } => DocumentStatus::Modified,
            PublicationState::Unpublished { .. } => DocumentStatus::Unpublished,
        }
    }
}

impl From<&DocumentSnapshot> for SnapshotHeader {
    fn from(s: &DocumentSnapshot) -> Self {
        Self {
            id: s.id,
            document_type_id: s.document_type_id.clone(),
            document_id: s.document_id,
            revision: s.revision,
            created_at: s.created_at,
            created_by: s.created_by.clone(),
            reason: s.reason,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{create_attr_id, create_doc_type_id, create_user_id};

    #[test]
    fn test_document_status_from_publication_state() {
        let now = Utc::now();
        let actor = ActorId::User(create_user_id("alice"));

        let draft = PublicationState::Draft;
        assert_eq!(DocumentStatus::from(&draft), DocumentStatus::Draft);

        let published = PublicationState::Published {
            revision: 2,
            published_at: now,
            published_by: actor.clone(),
        };
        assert_eq!(DocumentStatus::from(&published), DocumentStatus::Published);

        let modified = PublicationState::Modified {
            live_revision: 2,
            published_at: now,
            published_by: actor.clone(),
            draft_version: 4,
        };
        assert_eq!(DocumentStatus::from(&modified), DocumentStatus::Modified);

        let unpublished = PublicationState::Unpublished {
            last_published_revision: 2,
            unpublished_at: now,
            unpublished_by: actor,
        };
        assert_eq!(
            DocumentStatus::from(&unpublished),
            DocumentStatus::Unpublished
        );
    }

    #[test]
    fn test_snapshot_header_from_document_snapshot() {
        let now = Utc::now();
        let actor = ActorId::User(create_user_id("bob"));
        let doc_type = create_doc_type_id("article");
        let doc_id = DocumentInstanceId::new(Uuid::now_v7());
        let snap_id = SnapshotId::new(Uuid::now_v7());

        let snapshot = DocumentSnapshot {
            id: snap_id,
            document_type_id: doc_type.clone(),
            document_id: doc_id,
            revision: 3,
            fields: HashMap::new(),
            relations: HashMap::new(),
            created_at: now,
            created_by: actor.clone(),
            reason: SnapshotReason::Publish,
        };

        let header = SnapshotHeader::from(&snapshot);

        assert_eq!(header.id, snap_id);
        assert_eq!(header.document_type_id, doc_type);
        assert_eq!(header.document_id, doc_id);
        assert_eq!(header.revision, 3);
        assert_eq!(header.created_at, now);
        assert_eq!(header.created_by, actor);
        assert_eq!(header.reason, SnapshotReason::Publish);
    }

    #[test]
    fn test_enrich_relational_display() {
        let now = Utc::now();
        let actor = ActorId::User(create_user_id("alice"));
        let doc_type = create_doc_type_id("article");
        let doc_id = DocumentInstanceId::new(Uuid::now_v7());
        let title_attr = create_attr_id("title");
        let author_attr = create_attr_id("author");

        let mut display_values = IndexMap::new();
        display_values.insert(title_attr.clone(), "My Post".to_string());

        let mut header = DocumentHeader {
            id: doc_id,
            document_type_id: doc_type,
            display_values,
            display_title: "My Post".to_string(),
            relational_display_target: Some((
                author_attr.clone(),
                DocumentInstanceId::new(Uuid::now_v7()),
            )),
            status: DocumentStatus::Draft,
            slug: None,
            revision: None,
            draft_version: Some(1),
            created_at: now,
            created_by: actor.clone(),
            updated_at: now,
            updated_by: actor,
            published_at: None,
            published_by: None,
        };

        let effective_fields = vec![title_attr.clone(), author_attr.clone()];
        header.enrich_relational_display(&effective_fields, &author_attr, "Jane Doe");

        assert_eq!(
            header.display_values.get(&title_attr),
            Some(&"My Post".to_string())
        );
        assert_eq!(
            header.display_values.get(&author_attr),
            Some(&"Jane Doe".to_string())
        );
        assert_eq!(header.display_title, "My Post — Jane Doe");
    }

    #[test]
    fn test_version_view_default_and_serde() {
        assert_eq!(VersionView::default(), VersionView::Published);

        let json_pub = serde_json::to_string(&VersionView::Published).unwrap();
        assert_eq!(json_pub, "\"published\"");
        let deserialized_pub: VersionView = serde_json::from_str(&json_pub).unwrap();
        assert_eq!(deserialized_pub, VersionView::Published);

        let json_work = serde_json::to_string(&VersionView::Working).unwrap();
        assert_eq!(json_work, "\"working\"");
        let deserialized_work: VersionView = serde_json::from_str(&json_work).unwrap();
        assert_eq!(deserialized_work, VersionView::Working);
    }

    #[test]
    fn test_unpublish_draft_resolution_default_and_serde() {
        assert_eq!(
            UnpublishDraftResolution::default(),
            UnpublishDraftResolution::KeepDraft
        );

        let json_keep = serde_json::to_string(&UnpublishDraftResolution::KeepDraft).unwrap();
        assert_eq!(json_keep, "\"keep_draft\"");
        let deserialized_keep: UnpublishDraftResolution = serde_json::from_str(&json_keep).unwrap();
        assert_eq!(deserialized_keep, UnpublishDraftResolution::KeepDraft);

        let json_replace =
            serde_json::to_string(&UnpublishDraftResolution::ReplaceWithPublished).unwrap();
        assert_eq!(json_replace, "\"replace_with_published\"");
        let deserialized_replace: UnpublishDraftResolution =
            serde_json::from_str(&json_replace).unwrap();
        assert_eq!(
            deserialized_replace,
            UnpublishDraftResolution::ReplaceWithPublished
        );
    }
}
