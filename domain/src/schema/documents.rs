use std::{borrow::Borrow, collections::HashSet, hash::Hash};

use indexmap::IndexSet;
use nutype::nutype;
use serde::{Deserialize, Serialize};

use crate::{
    common::DisplayName,
    errors::DomainError,
    schema::{
        attributes::{AttributeId, FieldDefinition, RelationDefinition, RelationType},
        types::{FieldType, PrimitiveType},
    },
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
    pub display_fields: Vec<AttributeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentTypeOptions {
    pub draft_and_publish: bool,
}

impl DocumentType {
    /// Validates that `display_fields` satisfies schema registration invariants:
    /// 1. Maximum 3 display fields total.
    /// 2. No duplicate attributes.
    /// 3. Attributes must exist in `fields` (excluding `Json`) or in `relations` (`HasOne` / `BelongsToOne`).
    /// 4. Maximum 1 relational display field.
    pub fn validate_display_fields(&self) -> Result<(), DomainError> {
        // Rule 1: Max 3 Total
        if self.info.display_fields.len() > 3 {
            return Err(DomainError::InvalidSchemaDisplayField {
                type_id: self.id.clone(),
                reason: "at most 3 display_fields are allowed".into(),
            });
        }

        // Rule 2: No Duplicates
        let mut seen = HashSet::new();
        for attr_id in &self.info.display_fields {
            if !seen.insert(attr_id) {
                return Err(DomainError::InvalidSchemaDisplayField {
                    type_id: self.id.clone(),
                    reason: format!("duplicate display field '{attr_id}'"),
                });
            }
        }

        // Rules 3 & 4: Allowed Field / Relation Types & Max 1 Relational Display Field
        let mut relational_count = 0;
        for attr_id in &self.info.display_fields {
            if let Some(field_def) = self.fields.get(attr_id) {
                if matches!(field_def.field_type, FieldType::Json) {
                    return Err(DomainError::InvalidSchemaDisplayField {
                        type_id: self.id.clone(),
                        reason: format!(
                            "field '{attr_id}' has type Json which cannot be used as a display field"
                        ),
                    });
                }
            } else if let Some(rel_def) = self.relations.get(attr_id) {
                match rel_def.relation_type {
                    RelationType::HasOne | RelationType::BelongsToOne => {
                        relational_count += 1;
                        if relational_count > 1 {
                            return Err(DomainError::InvalidSchemaDisplayField {
                                type_id: self.id.clone(),
                                reason: "at most 1 relational display field is allowed".into(),
                            });
                        }
                    }
                    RelationType::HasMany | RelationType::BelongsToMany => {
                        return Err(DomainError::InvalidSchemaDisplayField {
                            type_id: self.id.clone(),
                            reason: format!(
                                "relation '{attr_id}' has multi-cardinality type which cannot be used as a display field"
                            ),
                        });
                    }
                }
            } else {
                return Err(DomainError::InvalidSchemaDisplayField {
                    type_id: self.id.clone(),
                    reason: format!("attribute '{attr_id}' is not declared in fields or relations"),
                });
            }
        }

        Ok(())
    }

    /// Returns the configured `display_fields`, or falls back to the first declared
    /// `Text`, `Uid`, or `LocalizedText` field in `self.fields`.
    pub fn effective_display_fields(&self) -> Vec<AttributeId> {
        if !self.info.display_fields.is_empty() {
            return self.info.display_fields.clone();
        }

        self.fields
            .iter()
            .find(|f| {
                matches!(
                    f.field_type,
                    FieldType::Primitive(PrimitiveType::Text | PrimitiveType::Uid)
                        | FieldType::LocalizedText
                )
            })
            .map(|f| vec![f.id.clone()])
            .unwrap_or_default()
    }

    /// Returns the singular `RelationDefinition` if `self.info.display_fields` contains
    /// a singular relation attribute (`HasOne` or `BelongsToOne`).
    pub fn relational_display_field(&self) -> Option<&RelationDefinition> {
        self.info.display_fields.iter().find_map(|attr_id| {
            self.relations.get(attr_id).filter(|rel| {
                matches!(
                    rel.relation_type,
                    RelationType::HasOne | RelationType::BelongsToOne
                )
            })
        })
    }
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

    #[test]
    fn test_display_fields_valid_up_to_three_fields_including_one_relation() {
        use crate::test_support::{test_attr_id, test_doc_type_id, DocumentTypeBuilder, FieldDefinitionBuilder};

        let f1 = FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text)).build();
        let f2 = FieldDefinitionBuilder::new("year", FieldType::Primitive(PrimitiveType::Integer(crate::schema::types::IntegerSize::I32))).build();
        let rel = RelationDefinition {
            id: test_attr_id("author"),
            relation_type: RelationType::HasOne,
            target: test_doc_type_id("author"),
        };

        let doc_type = DocumentTypeBuilder::collection("book")
            .with_field(f1)
            .with_field(f2)
            .with_relation(rel)
            .with_display_fields(vec![test_attr_id("title"), test_attr_id("year"), test_attr_id("author")])
            .build();

        assert!(doc_type.validate_display_fields().is_ok());
        assert_eq!(
            doc_type.effective_display_fields(),
            vec![test_attr_id("title"), test_attr_id("year"), test_attr_id("author")]
        );
        assert_eq!(
            doc_type.relational_display_field().map(|r| r.id.clone()),
            Some(test_attr_id("author"))
        );
    }

    #[test]
    fn test_display_fields_rejects_more_than_three() {
        use crate::test_support::{test_attr_id, DocumentTypeBuilder, FieldDefinitionBuilder};

        let f1 = FieldDefinitionBuilder::new("f1", FieldType::Primitive(PrimitiveType::Text)).build();
        let f2 = FieldDefinitionBuilder::new("f2", FieldType::Primitive(PrimitiveType::Text)).build();
        let f3 = FieldDefinitionBuilder::new("f3", FieldType::Primitive(PrimitiveType::Text)).build();
        let f4 = FieldDefinitionBuilder::new("f4", FieldType::Primitive(PrimitiveType::Text)).build();

        let doc_type = DocumentTypeBuilder::collection("test-doc")
            .with_field(f1)
            .with_field(f2)
            .with_field(f3)
            .with_field(f4)
            .with_display_fields(vec![
                test_attr_id("f1"),
                test_attr_id("f2"),
                test_attr_id("f3"),
                test_attr_id("f4"),
            ])
            .build();

        let err = doc_type.validate_display_fields().unwrap_err();
        assert!(matches!(err, DomainError::InvalidSchemaDisplayField { .. }));
        assert!(err.to_string().contains("at most 3"));
    }

    #[test]
    fn test_display_fields_rejects_duplicates() {
        use crate::test_support::{test_attr_id, DocumentTypeBuilder, FieldDefinitionBuilder};

        let f1 = FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text)).build();

        let doc_type = DocumentTypeBuilder::collection("test-doc")
            .with_field(f1)
            .with_display_fields(vec![test_attr_id("title"), test_attr_id("title")])
            .build();

        let err = doc_type.validate_display_fields().unwrap_err();
        assert!(matches!(err, DomainError::InvalidSchemaDisplayField { .. }));
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn test_display_fields_rejects_more_than_one_relation() {
        use crate::test_support::{test_attr_id, test_doc_type_id, DocumentTypeBuilder, FieldDefinitionBuilder};

        let f1 = FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text)).build();
        let rel1 = RelationDefinition {
            id: test_attr_id("author"),
            relation_type: RelationType::HasOne,
            target: test_doc_type_id("author"),
        };
        let rel2 = RelationDefinition {
            id: test_attr_id("publisher"),
            relation_type: RelationType::BelongsToOne,
            target: test_doc_type_id("publisher"),
        };

        let doc_type = DocumentTypeBuilder::collection("book")
            .with_field(f1)
            .with_relation(rel1)
            .with_relation(rel2)
            .with_display_fields(vec![
                test_attr_id("title"),
                test_attr_id("author"),
                test_attr_id("publisher"),
            ])
            .build();

        let err = doc_type.validate_display_fields().unwrap_err();
        assert!(matches!(err, DomainError::InvalidSchemaDisplayField { .. }));
        assert!(err.to_string().contains("at most 1 relational display field"));
    }

    #[test]
    fn test_display_fields_rejects_multi_cardinality_relations() {
        use crate::test_support::{test_attr_id, test_doc_type_id, DocumentTypeBuilder};

        let rel_many = RelationDefinition {
            id: test_attr_id("tags"),
            relation_type: RelationType::HasMany,
            target: test_doc_type_id("tag"),
        };

        let doc_type = DocumentTypeBuilder::collection("article")
            .with_relation(rel_many)
            .with_display_fields(vec![test_attr_id("tags")])
            .build();

        let err = doc_type.validate_display_fields().unwrap_err();
        assert!(matches!(err, DomainError::InvalidSchemaDisplayField { .. }));
        assert!(err.to_string().contains("multi-cardinality"));
    }

    #[test]
    fn test_display_fields_rejects_json_field() {
        use crate::test_support::{test_attr_id, DocumentTypeBuilder, FieldDefinitionBuilder};

        let f_json = FieldDefinitionBuilder::new("metadata", FieldType::Json).build();

        let doc_type = DocumentTypeBuilder::collection("record")
            .with_field(f_json)
            .with_display_fields(vec![test_attr_id("metadata")])
            .build();

        let err = doc_type.validate_display_fields().unwrap_err();
        assert!(matches!(err, DomainError::InvalidSchemaDisplayField { .. }));
        assert!(err.to_string().contains("Json"));
    }

    #[test]
    fn test_display_fields_rejects_undeclared_attribute() {
        use crate::test_support::{test_attr_id, DocumentTypeBuilder};

        let doc_type = DocumentTypeBuilder::collection("record")
            .with_display_fields(vec![test_attr_id("missing")])
            .build();

        let err = doc_type.validate_display_fields().unwrap_err();
        assert!(matches!(err, DomainError::InvalidSchemaDisplayField { .. }));
        assert!(err.to_string().contains("not declared"));
    }

    #[test]
    fn test_effective_display_fields_fallback() {
        use crate::test_support::{test_attr_id, DocumentTypeBuilder, FieldDefinitionBuilder};

        // Fallback to first text field when display_fields is empty
        let f_int = FieldDefinitionBuilder::new("counter", FieldType::Primitive(PrimitiveType::Integer(crate::schema::types::IntegerSize::I64))).build();
        let f_text = FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text)).build();

        let doc_type = DocumentTypeBuilder::collection("item")
            .with_field(f_int)
            .with_field(f_text)
            .build();

        assert_eq!(doc_type.effective_display_fields(), vec![test_attr_id("title")]);
        assert!(doc_type.relational_display_field().is_none());

        // Localized text fallback
        let f_loc = FieldDefinitionBuilder::new("heading", FieldType::LocalizedText).build();
        let doc_loc = DocumentTypeBuilder::collection("item")
            .with_field(f_loc)
            .build();
        assert_eq!(doc_loc.effective_display_fields(), vec![test_attr_id("heading")]);

        // No eligible text field -> empty vec
        let f_bool = FieldDefinitionBuilder::new("is-active", FieldType::Primitive(PrimitiveType::Boolean)).build();
        let doc_no_text = DocumentTypeBuilder::collection("item")
            .with_field(f_bool)
            .build();
        assert!(doc_no_text.effective_display_fields().is_empty());
    }
}
