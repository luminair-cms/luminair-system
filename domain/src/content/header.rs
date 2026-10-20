use std::collections::HashMap;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use nutype::nutype;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::ActorId;
use crate::content::instance::{DocumentInstanceId, PublicationState, ResolvedRelation};
use crate::content::values::ContentValue;
use crate::schema::{AttributeId, DocumentTypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Draft,
    Published,
    Modified,
    Unpublished,
}

impl From<&PublicationState> for DocumentStatus {
    fn from(state: &PublicationState) -> Self {
        match state {
            PublicationState::Draft { .. } => DocumentStatus::Draft,
            PublicationState::Published { .. } => DocumentStatus::Published,
            PublicationState::Modified { .. } => DocumentStatus::Modified,
            PublicationState::Unpublished { .. } => DocumentStatus::Unpublished,
        }
    }
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
    Checkpoint,
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
    use crate::test_support::{test_doc_type_id, test_user_id};

    #[test]
    fn test_document_status_from_publication_state() {
        let now = Utc::now();
        let actor = ActorId::User(test_user_id("alice"));

        let draft = PublicationState::Draft {
            last_published_revision: None,
        };
        assert_eq!(DocumentStatus::from(&draft), DocumentStatus::Draft);

        let draft_prev = PublicationState::Draft {
            last_published_revision: Some(1),
        };
        assert_eq!(DocumentStatus::from(&draft_prev), DocumentStatus::Draft);

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
        assert_eq!(DocumentStatus::from(&unpublished), DocumentStatus::Unpublished);
    }

    #[test]
    fn test_snapshot_header_from_document_snapshot() {
        let now = Utc::now();
        let actor = ActorId::User(test_user_id("bob"));
        let doc_type = test_doc_type_id("article");
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
}
