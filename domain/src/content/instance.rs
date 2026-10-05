use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use nutype::nutype;
use uuid::Uuid;

use crate::auth::UserId;
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

/*
Found issues for future investigation:
Public mutable fields: External callers can mutate instance.content.fields directly, bypassing version increments (touch()) and publication state transitions.
Missing set_field / get_field methods: The aggregate does not provide convenient domain methods to get or update field values.
Leaked DB detail (db_row_id): db_row_id has no explanation, no domain semantics, and is only initialized to None. If this is an infrastructure surrogate key, it does not belong in the domain aggregate.
 */

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInstance {
    pub id: DocumentInstanceId,
    pub db_row_id: Option<DocumentInstanceId>,
    pub document_type_id: DocumentTypeId,
    pub content: DocumentContent,
    pub relations: HashMap<AttributeId, Vec<ResolvedRelation>>,
    pub populated_relations: HashMap<AttributeId, Vec<DocumentInstance>>,
    pub audit: AuditTrail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentContent {
    pub fields: HashMap<AttributeId, ContentValue>,
    pub publication_state: PublicationState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicationState {
    Draft {
        last_published_revision: Option<u32>,
    },
    Published {
        revision: u32,
        published_at: DateTime<Utc>,
        published_by: Option<UserId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTrail {
    pub created_at: DateTime<Utc>,
    pub created_by: Option<UserId>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<UserId>,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRelation {
    pub attribute_id: AttributeId,
    pub target_instance_id: DocumentInstanceId,
}

impl DocumentInstance {
    pub fn new(type_id: DocumentTypeId, by: Option<UserId>, now: DateTime<Utc>) -> Self {
        Self {
            id: DocumentInstanceId::new(Uuid::now_v7()),
            db_row_id: None,
            document_type_id: type_id,
            content: DocumentContent {
                fields: HashMap::new(),
                publication_state: PublicationState::Draft {
                    last_published_revision: None,
                },
            },
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
    pub fn publish(&mut self, by: Option<UserId>, now: DateTime<Utc>) -> Result<u32, DomainError> {
        let new_revision = match self.content.publication_state {
            PublicationState::Draft {
                last_published_revision,
            } => last_published_revision.unwrap_or(0) + 1,
            PublicationState::Published { revision, .. } => revision + 1,
        };

        self.content.publication_state = PublicationState::Published {
            revision: new_revision,
            published_at: now,
            published_by: by.clone(),
        };

        self.touch(by, now);

        Ok(new_revision)
    }

    /// Transitions a published document back into a draft, recording the last published revision.
    pub fn unpublish(&mut self, by: Option<UserId>, now: DateTime<Utc>) -> Result<(), DomainError> {
        match self.content.publication_state {
            PublicationState::Published { revision, .. } => {
                self.content.publication_state = PublicationState::Draft {
                    last_published_revision: Some(revision),
                };
                self.touch(by, now);
                Ok(())
            }
            PublicationState::Draft { .. } => Err(DomainError::InvalidStateTransition {
                reason: "cannot unpublish a document that is already a draft".to_string(),
            }),
        }
    }

    /// Records an edit/touch on the instance, updating updated_at and incrementing version.
    pub fn touch(&mut self, by: Option<UserId>, now: DateTime<Utc>) {
        self.audit.updated_at = now;
        self.audit.updated_by = by;
        self.audit.version += 1;
    }

    pub fn is_owned_by(&self, user_id: &UserId) -> bool {
        self.audit.created_by.as_ref() == Some(user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use crate::test_support::{test_attr_id, test_doc_type_id, test_user_id};

    fn make_test_instance() -> (DocumentInstance, UserId, DateTime<Utc>) {
        let user = test_user_id("author-1");
        let now = Utc::now();
        let instance = DocumentInstance::new(test_doc_type_id("article"), Some(user.clone()), now);
        (instance, user, now)
    }

    #[test]
    fn test_publish_from_draft_increments_revision_and_updates_audit() {
        let (mut instance, user, t0) = make_test_instance();
        let t1 = t0 + Duration::seconds(10);

        let rev = instance.publish(Some(user.clone()), t1).unwrap();
        assert_eq!(rev, 1);
        assert_eq!(instance.audit.version, 2);
        assert_eq!(instance.audit.updated_at, t1);
        assert_eq!(instance.audit.updated_by, Some(user.clone()));

        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Published {
                revision: 1,
                published_at,
                published_by,
            } if published_at == t1 && published_by == Some(user)
        ));
    }

    #[test]
    fn test_publish_preserves_and_advances_previous_published_revision() {
        let (mut instance, user, t0) = make_test_instance();
        let t1 = t0 + Duration::seconds(10);
        let t2 = t0 + Duration::seconds(20);
        let t3 = t0 + Duration::seconds(30);

        // Publish revision 1
        instance.publish(Some(user.clone()), t1).unwrap();
        // Unpublish back to draft
        instance.unpublish(Some(user.clone()), t2).unwrap();
        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Draft {
                last_published_revision: Some(1)
            }
        ));
        assert_eq!(instance.audit.updated_by, Some(user.clone()));
        assert_eq!(instance.audit.updated_at, t2);

        // Re-publish should advance to revision 2
        let rev = instance.publish(Some(user), t3).unwrap();
        assert_eq!(rev, 2);
        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Published { revision: 2, .. }
        ));
    }

    #[test]
    fn test_publish_from_published_state_advances_revision() {
        let (mut instance, user, t0) = make_test_instance();
        instance.publish(Some(user.clone()), t0).unwrap();
        let t1 = t0 + Duration::seconds(15);

        let rev = instance.publish(Some(user), t1).unwrap();
        assert_eq!(rev, 2);
    }

    #[test]
    fn test_unpublish_from_published_transitions_to_draft_with_last_revision() {
        let (mut instance, user, t0) = make_test_instance();
        instance.publish(Some(user.clone()), t0).unwrap();
        let t1 = t0 + Duration::seconds(10);

        instance.unpublish(Some(user.clone()), t1).unwrap();
        assert!(matches!(
            instance.content.publication_state,
            PublicationState::Draft {
                last_published_revision: Some(1)
            }
        ));
        assert_eq!(instance.audit.updated_at, t1);
        assert_eq!(instance.audit.updated_by, Some(user));
    }

    #[test]
    fn test_unpublish_from_draft_fails_with_invalid_state_transition() {
        let (mut instance, user, t0) = make_test_instance();
        let result = instance.unpublish(Some(user), t0);
        assert!(matches!(
            result,
            Err(DomainError::InvalidStateTransition { .. })
        ));
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
    }

    #[test]
    fn test_touch_updates_timestamp_actor_and_increments_version() {
        let (mut instance, _, t0) = make_test_instance();
        assert_eq!(instance.audit.version, 1);
        let editor = test_user_id("editor");
        let t1 = t0 + Duration::seconds(45);

        instance.touch(Some(editor.clone()), t1);
        assert_eq!(instance.audit.version, 2);
        assert_eq!(instance.audit.updated_at, t1);
        assert_eq!(instance.audit.updated_by, Some(editor));
    }
}
