use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use nutype::nutype;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{ActorId, UserId};
use crate::content::header::DocumentSnapshot;
use crate::content::ContentValue;
use crate::errors::DomainError;
use crate::schema::{AttributeId, DocumentTypeId};

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
pub struct DocumentInstanceId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInstance {
    pub id: DocumentInstanceId,
    pub document_type_id: DocumentTypeId,
    pub content: DocumentContent,
    pub relations: HashMap<AttributeId, Vec<ResolvedRelation>>,
    pub populated_relations: HashMap<AttributeId, Vec<DocumentInstance>>,
    pub audit: AuditTrail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentContent {
    fields: HashMap<AttributeId, ContentValue>,
    pub publication_state: PublicationState,
}

impl DocumentContent {
    pub fn new(
        fields: HashMap<AttributeId, ContentValue>,
        publication_state: PublicationState,
    ) -> Self {
        Self {
            fields,
            publication_state,
        }
    }

    pub fn fields(&self) -> &HashMap<AttributeId, ContentValue> {
        &self.fields
    }

    pub fn get_field(&self, attr_id: &AttributeId) -> Option<&ContentValue> {
        self.fields.get(attr_id)
    }

    pub fn set_field(&mut self, attr_id: AttributeId, value: ContentValue) {
        self.fields.insert(attr_id, value);
    }

    pub fn remove_field(&mut self, attr_id: &AttributeId) -> Option<ContentValue> {
        self.fields.remove(attr_id)
    }

    pub fn replace_fields(&mut self, fields: HashMap<AttributeId, ContentValue>) {
        self.fields = fields;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublicationState {
    Draft {
        last_published_revision: Option<u32>,
    },
    Published {
        revision: u32,
        published_at: DateTime<Utc>,
        published_by: ActorId,
    },
    Modified {
        live_revision: u32,
        published_at: DateTime<Utc>,
        published_by: ActorId,
        draft_version: u32,
    },
    Unpublished {
        last_published_revision: u32,
        unpublished_at: DateTime<Utc>,
        unpublished_by: ActorId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTrail {
    pub created_at: DateTime<Utc>,
    pub created_by: ActorId,
    pub updated_at: DateTime<Utc>,
    pub updated_by: ActorId,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRelation {
    pub attribute_id: AttributeId,
    pub target_instance_id: DocumentInstanceId,
}

impl DocumentInstance {
    pub fn new(type_id: DocumentTypeId, by: impl Into<ActorId>, now: DateTime<Utc>) -> Self {
        let by = by.into();
        Self {
            id: DocumentInstanceId::new(Uuid::now_v7()),
            document_type_id: type_id,
            content: DocumentContent::new(
                HashMap::new(),
                PublicationState::Draft {
                    last_published_revision: None,
                },
            ),
            relations: HashMap::new(),
            populated_relations: HashMap::new(),
            audit: AuditTrail {
                created_at: now,
                created_by: by.clone(),
                updated_at: now,
                updated_by: by,
                version: 1,
            },
        }
    }

    /// Reconstitutes a `DocumentInstance` from persistence state without incrementing version or generating new IDs.
    pub fn reconstitute(
        id: DocumentInstanceId,
        document_type_id: DocumentTypeId,
        content: DocumentContent,
        relations: HashMap<AttributeId, Vec<ResolvedRelation>>,
        audit: AuditTrail,
    ) -> Self {
        Self {
            id,
            document_type_id,
            content,
            relations,
            populated_relations: HashMap::new(),
            audit,
        }
    }

    pub fn fields(&self) -> &HashMap<AttributeId, ContentValue> {
        self.content.fields()
    }

    pub fn get_field(&self, attr_id: &AttributeId) -> Option<&ContentValue> {
        self.content.get_field(attr_id)
    }

    pub fn set_field(&mut self, attr_id: AttributeId, value: ContentValue) {
        self.content.set_field(attr_id, value);
    }

    pub fn remove_field(&mut self, attr_id: &AttributeId) -> Option<ContentValue> {
        self.content.remove_field(attr_id)
    }

    pub fn replace_fields(&mut self, fields: HashMap<AttributeId, ContentValue>) {
        self.content.replace_fields(fields);
    }

    /// Returns a new instance with populated related document instances attached.
    pub fn with_populated_relations(
        mut self,
        populated: HashMap<AttributeId, Vec<DocumentInstance>>,
    ) -> Self {
        self.populated_relations = populated;
        self
    }

    /// Replaces all relations for the given attribute with the specified target IDs (deduplicated).
    pub fn set_relations(&mut self, attr: AttributeId, target_ids: Vec<DocumentInstanceId>) {
        let mut seen = HashSet::new();
        let mut list = Vec::new();
        for target_id in target_ids {
            if seen.insert(target_id) {
                list.push(ResolvedRelation {
                    attribute_id: attr.clone(),
                    target_instance_id: target_id,
                });
            }
        }
        self.relations.insert(attr, list);
    }

    /// Appends the specified target IDs to the existing relations for the given attribute, ignoring duplicates.
    pub fn connect_relations(&mut self, attr: AttributeId, target_ids: Vec<DocumentInstanceId>) {
        let list = self.relations.entry(attr.clone()).or_default();
        let mut seen: HashSet<_> = list.iter().map(|r| r.target_instance_id).collect();
        for target_id in target_ids {
            if seen.insert(target_id) {
                list.push(ResolvedRelation {
                    attribute_id: attr.clone(),
                    target_instance_id: target_id,
                });
            }
        }
    }

    /// Removes the specified target IDs from the relations of the given attribute.
    pub fn disconnect_relations(&mut self, attr: &AttributeId, target_ids: &[DocumentInstanceId]) {
        if let Some(list) = self.relations.get_mut(attr) {
            let targets_to_remove: HashSet<_> = target_ids.iter().copied().collect();
            list.retain(|r| !targets_to_remove.contains(&r.target_instance_id));
        }
    }

    /// Clears all relations for the given attribute.
    pub fn unset_relations(&mut self, attr: &AttributeId) {
        self.relations.remove(attr);
    }

    /// Publishes the document instance, advancing its revision and returning the new revision number.
    pub fn publish(&mut self, by: impl Into<ActorId>, now: DateTime<Utc>) -> Result<u32, DomainError> {
        let by = by.into();
        let new_revision = match self.content.publication_state {
            PublicationState::Draft {
                last_published_revision,
            } => last_published_revision.unwrap_or(0) + 1,
            PublicationState::Published { revision, .. } => revision + 1,
            PublicationState::Modified { live_revision, .. } => live_revision + 1,
            PublicationState::Unpublished {
                last_published_revision,
                ..
            } => last_published_revision + 1,
        };

        self.content.publication_state = PublicationState::Published {
            revision: new_revision,
            published_at: now,
            published_by: by.clone(),
        };

        self.touch(by, now);

        Ok(new_revision)
    }

    /// Transitions a published or modified document into unpublished state, recording the last revision.
    pub fn unpublish(&mut self, by: impl Into<ActorId>, now: DateTime<Utc>) -> Result<(), DomainError> {
        let by = by.into();
        match self.content.publication_state {
            PublicationState::Published { revision, .. }
            | PublicationState::Modified {
                live_revision: revision,
                ..
            } => {
                self.content.publication_state = PublicationState::Unpublished {
                    last_published_revision: revision,
                    unpublished_at: now,
                    unpublished_by: by.clone(),
                };
                self.touch(by, now);
                Ok(())
            }
            PublicationState::Draft { .. } | PublicationState::Unpublished { .. } => {
                Err(DomainError::InvalidStateTransition {
                    reason: "cannot unpublish a document that is not currently published".to_string(),
                })
            }
        }
    }

    /// Transitions a published document into modified state, updating draft_version and touching audit.
    pub fn edit_draft(&mut self, by: impl Into<ActorId>, now: DateTime<Utc>) {
        let by = by.into();
        match self.content.publication_state.clone() {
            PublicationState::Published {
                revision,
                published_at,
                published_by,
            } => {
                self.touch(by, now);
                self.content.publication_state = PublicationState::Modified {
                    live_revision: revision,
                    published_at,
                    published_by,
                    draft_version: self.audit.version,
                };
            }
            PublicationState::Modified {
                live_revision,
                published_at,
                published_by,
                ..
            } => {
                self.touch(by, now);
                self.content.publication_state = PublicationState::Modified {
                    live_revision,
                    published_at,
                    published_by,
                    draft_version: self.audit.version,
                };
            }
            PublicationState::Draft { .. } | PublicationState::Unpublished { .. } => {
                self.touch(by, now);
            }
        }
    }

    /// Discards a modified draft, restoring clean Published state without incrementing audit version.
    pub fn discard_draft(&mut self) -> Result<(), DomainError> {
        match self.content.publication_state.clone() {
            PublicationState::Modified {
                live_revision,
                published_at,
                published_by,
                ..
            } => {
                self.content.publication_state = PublicationState::Published {
                    revision: live_revision,
                    published_at,
                    published_by,
                };
                Ok(())
            }
            _ => Err(DomainError::InvalidStateTransition {
                reason: "cannot discard draft when document is not in Modified state".to_string(),
            }),
        }
    }

    /// Restores content and relations from a snapshot and marks the document as a modified draft.
    pub fn restore_snapshot(
        &mut self,
        snapshot: &DocumentSnapshot,
        by: impl Into<ActorId>,
        now: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        if snapshot.document_id != self.id || snapshot.document_type_id != self.document_type_id {
            return Err(DomainError::Validation(format!(
                "snapshot {}:{} does not belong to document {}:{}",
                snapshot.document_type_id, snapshot.document_id, self.document_type_id, self.id
            )));
        }
        self.replace_fields(snapshot.fields.clone());
        self.relations = snapshot.relations.clone();
        self.edit_draft(by, now);
        Ok(())
    }

    /// Records an edit/touch on the instance, updating updated_at and incrementing version.
    pub fn touch(&mut self, by: impl Into<ActorId>, now: DateTime<Utc>) {
        self.audit.updated_at = now;
        self.audit.updated_by = by.into();
        self.audit.version += 1;
    }

    pub fn is_owned_by(&self, user_id: &UserId) -> bool {
        self.audit.created_by.as_user() == Some(user_id)
    }

    pub fn is_owned_by_actor(&self, actor: &ActorId) -> bool {
        &self.audit.created_by == actor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use crate::content::header::{SnapshotId, SnapshotReason};
    use crate::content::values::{DomainValue, PrimitiveValue};
    use crate::test_support::{test_attr_id, test_doc_type_id, test_user_id};

    fn make_test_instance() -> (DocumentInstance, UserId, DateTime<Utc>) {
        let user = test_user_id("author-1");
        let now = Utc::now();
        let instance = DocumentInstance::new(test_doc_type_id("article"), user.clone(), now);
        (instance, user, now)
    }

    #[test]
    fn test_publish_from_draft_increments_revision_and_updates_audit() {
        let (mut instance, user, t0) = make_test_instance();
        let t1 = t0 + Duration::seconds(10);

        let rev = instance.publish(user.clone(), t1).unwrap();
        assert_eq!(rev, 1);
        assert_eq!(instance.audit.version, 2);
        assert_eq!(instance.audit.updated_at, t1);
        assert_eq!(instance.audit.updated_by, ActorId::User(user.clone()));

        assert!(matches!(
            &instance.content.publication_state,
            PublicationState::Published {
                revision: 1,
                published_at,
                published_by,
            } if *published_at == t1 && published_by == &ActorId::User(user)
        ));
    }

    #[test]
    fn test_publish_preserves_and_advances_previous_published_revision() {
        let (mut instance, user, t0) = make_test_instance();
        let t1 = t0 + Duration::seconds(10);
        let t2 = t0 + Duration::seconds(20);
        let t3 = t0 + Duration::seconds(30);

        // Publish revision 1
        instance.publish(user.clone(), t1).unwrap();
        // Unpublish to Unpublished
        instance.unpublish(user.clone(), t2).unwrap();
        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Unpublished {
                last_published_revision: 1,
                ..
            }
        ));
        assert_eq!(instance.audit.updated_by, ActorId::User(user.clone()));
        assert_eq!(instance.audit.updated_at, t2);

        // Re-publish should advance to revision 2
        let rev = instance.publish(user, t3).unwrap();
        assert_eq!(rev, 2);
        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Published { revision: 2, .. }
        ));
    }

    #[test]
    fn test_publish_from_published_state_advances_revision() {
        let (mut instance, user, t0) = make_test_instance();
        instance.publish(user.clone(), t0).unwrap();
        let t1 = t0 + Duration::seconds(15);

        let rev = instance.publish(user, t1).unwrap();
        assert_eq!(rev, 2);
    }

    #[test]
    fn test_unpublish_from_published_transitions_to_unpublished_with_last_revision() {
        let (mut instance, user, t0) = make_test_instance();
        instance.publish(user.clone(), t0).unwrap();
        let t1 = t0 + Duration::seconds(10);

        instance.unpublish(user.clone(), t1).unwrap();
        assert!(matches!(
            &instance.content.publication_state,
            PublicationState::Unpublished {
                last_published_revision: 1,
                unpublished_at,
                unpublished_by,
            } if *unpublished_at == t1 && unpublished_by == &ActorId::User(user.clone())
        ));
        assert_eq!(instance.audit.updated_at, t1);
        assert_eq!(instance.audit.updated_by, ActorId::User(user));
    }

    #[test]
    fn test_unpublish_from_draft_fails_with_invalid_state_transition() {
        let (mut instance, user, t0) = make_test_instance();
        let result = instance.unpublish(user, t0);
        assert!(matches!(
            result,
            Err(DomainError::InvalidStateTransition { .. })
        ));
    }

    #[test]
    fn test_edit_draft_and_discard_draft_lifecycle() {
        let (mut instance, user, t0) = make_test_instance();
        let t1 = t0 + Duration::seconds(10);
        let t2 = t0 + Duration::seconds(20);

        // Publish revision 1
        instance.publish(user.clone(), t1).unwrap();
        assert_eq!(instance.audit.version, 2);

        // Edit draft -> transitions to Modified, version bumps to 3
        instance.edit_draft(user.clone(), t2);
        assert_eq!(instance.audit.version, 3);
        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Modified {
                live_revision: 1,
                draft_version: 3,
                ..
            }
        ));

        // Discard draft -> transitions back to Published, version remains 3
        instance.discard_draft().unwrap();
        assert_eq!(instance.audit.version, 3);
        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Published { revision: 1, .. }
        ));

        // Discarding draft when not Modified returns error
        assert!(matches!(
            instance.discard_draft(),
            Err(DomainError::InvalidStateTransition { .. })
        ));
    }

    #[test]
    fn test_restore_snapshot() {
        let (mut instance, user, t0) = make_test_instance();
        let t1 = t0 + Duration::seconds(10);
        let t2 = t0 + Duration::seconds(20);

        instance.publish(user.clone(), t1).unwrap();
        assert_eq!(instance.audit.version, 2);

        let title_attr = test_attr_id("title");
        let mut snap_fields = HashMap::new();
        snap_fields.insert(
            title_attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Snapshot Title".into()))),
        );

        let snapshot = DocumentSnapshot {
            id: SnapshotId::new(Uuid::now_v7()),
            document_type_id: instance.document_type_id.clone(),
            document_id: instance.id,
            revision: 1,
            fields: snap_fields,
            relations: HashMap::new(),
            created_at: t1,
            created_by: ActorId::User(user.clone()),
            reason: SnapshotReason::Publish,
        };

        instance.restore_snapshot(&snapshot, user.clone(), t2).unwrap();
        assert_eq!(instance.audit.version, 3);
        assert_eq!(instance.audit.updated_at, t2);
        assert_eq!(
            instance.get_field(&title_attr),
            Some(&ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Snapshot Title".into()))))
        );
        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Modified {
                live_revision: 1,
                draft_version: 3,
                ..
            }
        ));

        // Foreign snapshot rejection
        let foreign_snapshot = DocumentSnapshot {
            id: SnapshotId::new(Uuid::now_v7()),
            document_type_id: instance.document_type_id.clone(),
            document_id: DocumentInstanceId::new(Uuid::now_v7()),
            revision: 1,
            fields: HashMap::new(),
            relations: HashMap::new(),
            created_at: t1,
            created_by: ActorId::User(user.clone()),
            reason: SnapshotReason::Publish,
        };
        let err = instance.restore_snapshot(&foreign_snapshot, user, t2);
        assert!(matches!(err, Err(DomainError::Validation(_))));
    }

    #[test]
    fn test_encapsulated_field_accessors() {
        let (mut instance, _, _) = make_test_instance();
        let title_attr = test_attr_id("title");
        let title_val = ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Rust".into())));

        assert!(instance.fields().is_empty());
        assert_eq!(instance.get_field(&title_attr), None);

        instance.set_field(title_attr.clone(), title_val.clone());
        assert_eq!(instance.get_field(&title_attr), Some(&title_val));
        assert_eq!(instance.fields().len(), 1);

        let removed = instance.remove_field(&title_attr);
        assert_eq!(removed, Some(title_val));
        assert_eq!(instance.get_field(&title_attr), None);

        let mut new_fields = HashMap::new();
        let desc_attr = test_attr_id("desc");
        let desc_val = ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Desc".into())));
        new_fields.insert(desc_attr.clone(), desc_val.clone());
        instance.replace_fields(new_fields);

        assert_eq!(instance.fields().len(), 1);
        assert_eq!(instance.get_field(&desc_attr), Some(&desc_val));
    }

    #[test]
    fn test_set_relations_deduplicates_target_ids() {
        let (mut instance, _, _) = make_test_instance();
        let attr = test_attr_id("categories");
        let id1 = DocumentInstanceId::new(Uuid::now_v7());
        let id2 = DocumentInstanceId::new(Uuid::now_v7());

        // Pass id1 twice to test deduplication
        instance.set_relations(attr.clone(), vec![id1, id2, id1]);
        let relations = instance.relations.get(&attr).unwrap();
        assert_eq!(relations.len(), 2);
        assert_eq!(relations[0].target_instance_id, id1);
        assert_eq!(relations[1].target_instance_id, id2);
    }

    #[test]
    fn test_connect_relations_appends_and_deduplicates() {
        let (mut instance, _, _) = make_test_instance();
        let attr = test_attr_id("tags");
        let id1 = DocumentInstanceId::new(Uuid::now_v7());
        let id2 = DocumentInstanceId::new(Uuid::now_v7());
        let id3 = DocumentInstanceId::new(Uuid::now_v7());

        instance.set_relations(attr.clone(), vec![id1, id2]);
        // Connect id2 (already present) and id3 (new)
        instance.connect_relations(attr.clone(), vec![id2, id3]);

        let relations = instance.relations.get(&attr).unwrap();
        assert_eq!(relations.len(), 3);
        let target_ids: Vec<_> = relations.iter().map(|r| r.target_instance_id).collect();
        assert_eq!(target_ids, vec![id1, id2, id3]);
    }

    #[test]
    fn test_disconnect_relations_removes_matching_targets_only() {
        let (mut instance, _, _) = make_test_instance();
        let attr = test_attr_id("tags");
        let id1 = DocumentInstanceId::new(Uuid::now_v7());
        let id2 = DocumentInstanceId::new(Uuid::now_v7());
        let id3 = DocumentInstanceId::new(Uuid::now_v7());

        instance.set_relations(attr.clone(), vec![id1, id2, id3]);
        instance.disconnect_relations(&attr, &[id2]);

        let relations = instance.relations.get(&attr).unwrap();
        let target_ids: Vec<_> = relations.iter().map(|r| r.target_instance_id).collect();
        assert_eq!(target_ids, vec![id1, id3]);
    }

    #[test]
    fn test_unset_relations_removes_attribute() {
        let (mut instance, _, _) = make_test_instance();
        let attr = test_attr_id("tags");
        let id1 = DocumentInstanceId::new(Uuid::now_v7());

        instance.set_relations(attr.clone(), vec![id1]);
        assert!(instance.relations.contains_key(&attr));

        instance.unset_relations(&attr);
        assert!(!instance.relations.contains_key(&attr));
    }

    #[test]
    fn test_is_owned_by_matching_and_non_matching_user() {
        let (instance, owner, _) = make_test_instance();
        let stranger = test_user_id("stranger");

        assert!(instance.is_owned_by(&owner));
        assert!(!instance.is_owned_by(&stranger));
        assert!(instance.is_owned_by_actor(&ActorId::User(owner)));
        assert!(!instance.is_owned_by_actor(&ActorId::User(stranger)));
    }

    #[test]
    fn test_touch_updates_timestamp_actor_and_increments_version() {
        let (mut instance, _, t0) = make_test_instance();
        assert_eq!(instance.audit.version, 1);
        let editor = test_user_id("editor");
        let t1 = t0 + Duration::seconds(45);

        instance.touch(editor.clone(), t1);
        assert_eq!(instance.audit.version, 2);
        assert_eq!(instance.audit.updated_at, t1);
        assert_eq!(instance.audit.updated_by, ActorId::User(editor));
    }

    #[test]
    fn test_reconstitute_preserves_state() {
        let doc_id = DocumentInstanceId::new(Uuid::now_v7());
        let doc_type = test_doc_type_id("article");
        let now = Utc::now();
        let actor = ActorId::User(test_user_id("creator"));
        let audit = AuditTrail {
            created_at: now,
            created_by: actor.clone(),
            updated_at: now,
            updated_by: actor,
            version: 7,
        };
        let content = DocumentContent::new(
            HashMap::new(),
            PublicationState::Draft {
                last_published_revision: Some(2),
            },
        );
        let instance = DocumentInstance::reconstitute(
            doc_id,
            doc_type.clone(),
            content,
            HashMap::new(),
            audit,
        );
        assert_eq!(instance.id, doc_id);
        assert_eq!(instance.document_type_id, doc_type);
        assert_eq!(instance.audit.version, 7);
        assert!(instance.relations.is_empty());
        assert!(instance.populated_relations.is_empty());
    }
}
