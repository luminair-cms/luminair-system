use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{DomainError, auth::UserId, content::{ContentValue, DocumentInstanceId}, schema::{AttributeId, DocumentTypeId}};


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
    pub fn unpublish(&mut self, now: DateTime<Utc>) -> Result<(), DomainError> {
        match self.content.publication_state {
            PublicationState::Published { revision, .. } => {
                self.content.publication_state = PublicationState::Draft {
                    last_published_revision: Some(revision),
                };
                self.audit.updated_at = now;
                self.audit.version += 1;
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
