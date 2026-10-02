use std::{borrow::Borrow, hash::Hash};

use indexmap::IndexSet;
use nutype::nutype;
use serde::{Deserialize, Serialize};

use crate::{
    common::DisplayName,
    schema::attributes::{FieldDefinition, RelationDefinition},
};

#[nutype(
    sanitize(trim),
    validate(
        not_empty,
        len_char_min = 2,
        len_char_max = 64,
        regex = r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$"
    ),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Display,
        Serialize,
        Deserialize,
        AsRef,
        Deref,
        Into
    )
)]
pub struct DocumentTypeId(String);

#[derive(Debug, Clone)]
pub struct DocumentType {
    pub id: DocumentTypeId,
    pub kind: DocumentKind,
    pub info: DocumentTypeInfo,
    pub options: DocumentTypeOptions,
    pub fields: IndexSet<FieldDefinition>,
    pub relations: IndexSet<RelationDefinition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocumentKind {
    Collection,
    SingleType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentTypeInfo {
    pub title: DisplayName,
    pub singular_name: DocumentTypeId,
    pub plural_name: DocumentTypeId,
    pub description: Option<DisplayName>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentTypeOptions {
    pub draft_and_publish: bool,
}

impl PartialEq for DocumentType {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for DocumentType {}

impl PartialEq<DocumentTypeId> for DocumentType {
    fn eq(&self, other: &DocumentTypeId) -> bool {
        self.id == *other
    }
}

impl Hash for DocumentType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Borrow<DocumentTypeId> for DocumentType {
    fn borrow(&self) -> &DocumentTypeId {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_type_id_valid() {
        assert!(DocumentTypeId::try_new("article").is_ok());
        assert!(DocumentTypeId::try_new("partner-booking-category").is_ok());
        assert!(DocumentTypeId::try_new("site-settings").is_ok());
    }

    #[test]
    fn test_document_type_id_rejects_underscores_and_uppercase() {
        assert!(DocumentTypeId::try_new("partner_category").is_err());
        assert!(DocumentTypeId::try_new("Article").is_err());
        assert!(DocumentTypeId::try_new("partnerBookingCategory").is_err());
    }

    #[test]
    fn test_document_type_id_rejects_invalid_hyphenation() {
        assert!(DocumentTypeId::try_new("-article").is_err());
        assert!(DocumentTypeId::try_new("article-").is_err());
        assert!(DocumentTypeId::try_new("article--content").is_err());
    }
}
