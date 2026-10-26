use std::collections::HashMap;

use indexmap::IndexSet;

use crate::errors::DomainError;
use crate::schema::types::{FieldType, PrimitiveType};
use crate::schema::{DocumentKind, DocumentType, DocumentTypeId};

#[derive(Debug, Clone, Default)]
pub struct SchemaRegistry {
    types: IndexSet<DocumentType>,
    by_name: HashMap<String, DocumentTypeId>,
}

impl SchemaRegistry {
    #[must_use]
    pub fn new(types: Vec<DocumentType>) -> Self {
        let types: IndexSet<DocumentType> = types.into_iter().collect();

        let mut by_name = HashMap::new();
        for dt in &types {
            let api_id = match dt.kind {
                DocumentKind::Singleton => dt.info.singular_name.as_ref().to_string(),
                DocumentKind::Collection => dt.info.plural_name.as_ref().to_string(),
            };
            by_name.insert(api_id, dt.id.clone());
        }

        Self { types, by_name }
    }

    /// Constructs a `SchemaRegistry` and validates all registered document types,
    /// including `display_fields`, field constraint applicability, and relation target existence.
    ///
    /// # Errors
    /// Returns [`DomainError`] if any document type schema definition is invalid.
    pub fn try_new(types: Vec<DocumentType>) -> Result<Self, DomainError> {
        let registry = Self::new(types);
        registry.validate()?;
        Ok(registry)
    }

    /// Validates schema-wide invariants across all registered document types:
    /// 1. Each document type's `display_fields` satisfy `validate_display_fields()`.
    /// 2. Each field's constraints are applicable to its `field_type`.
    /// 3. Each relation's `target` `DocumentTypeId` exists in the registry.
    ///
    /// # Errors
    /// Returns [`DomainError`] if validation fails.
    pub fn validate(&self) -> Result<(), DomainError> {
        for dt in &self.types {
            dt.validate_display_fields()?;

            for field in &dt.fields {
                if let FieldType::Primitive(PrimitiveType::Decimal { precision, scale }) =
                    field.field_type
                    && (precision == 0 || precision > 38 || scale > precision)
                {
                    return Err(DomainError::Validation(format!(
                        "invalid decimal definition (precision: {}, scale: {}) on field '{}' of document type '{}': precision must be in 1..=38 and scale <= precision",
                        precision, scale, field.id, dt.id
                    )));
                }

                for constraint in &field.constraints {
                    if !constraint.is_applicable_for(&field.field_type) {
                        return Err(DomainError::Validation(format!(
                            "constraint {:?} is not applicable to field '{}' of type {:?} on document type '{}'",
                            constraint, field.id, field.field_type, dt.id
                        )));
                    }
                }
            }

            for rel in &dt.relations {
                if !self.types.contains(&rel.target) {
                    return Err(DomainError::DocumentTypeNotFound(rel.target.clone()));
                }
            }
        }
        Ok(())
    }

    pub fn iterate(&self) -> impl Iterator<Item = &DocumentType> {
        self.types.iter()
    }

    #[must_use]
    pub fn get(&self, id: &DocumentTypeId) -> Option<&DocumentType> {
        self.types.get(id)
    }

    #[must_use]
    pub fn find_by_name(&self, plural_name: &str) -> Option<&DocumentType> {
        self.by_name
            .get(plural_name)
            .and_then(|id| self.types.get(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::RelationType;
    use crate::schema::attributes::RelationDefinition;
    use crate::schema::types::{FieldType, PrimitiveType};
    use crate::test_support::{
        DocumentTypeBuilder, FieldDefinitionBuilder, create_attr_id, create_doc_type_id, text_field,
    };

    #[test]
    fn test_schema_registry_indexes_collection_by_plural_and_single_by_singular() {
        let coll = DocumentTypeBuilder::collection("article").build();
        let single = DocumentTypeBuilder::single("site-settings").build();
        let coll_id = coll.id.clone();
        let single_id = single.id.clone();

        let registry = SchemaRegistry::new(vec![coll, single]);

        // Collection is found by plural name
        assert_eq!(registry.find_by_name("articles").unwrap().id, coll_id);
        // SingleType is found by singular name
        assert_eq!(
            registry.find_by_name("site-settings").unwrap().id,
            single_id
        );

        // Found by ID
        assert_eq!(registry.get(&coll_id).unwrap().id, coll_id);
        assert_eq!(registry.get(&single_id).unwrap().id, single_id);

        // Non-existent lookups return None
        assert!(registry.find_by_name("non-existent").is_none());
        assert!(registry.get(&create_doc_type_id("missing")).is_none());
    }

    #[test]
    fn test_schema_registry_iterate_returns_all_types() {
        let t1 = DocumentTypeBuilder::collection("article").build();
        let t2 = DocumentTypeBuilder::collection("author").build();

        let registry = SchemaRegistry::new(vec![t1, t2]);
        let names: Vec<_> = registry.iterate().map(|t| t.id.as_ref()).collect();
        assert_eq!(names, vec!["article", "author"]);
    }

    #[test]
    fn test_schema_registry_try_new_valid() {
        let author = DocumentTypeBuilder::collection("author")
            .with_field(text_field("name"))
            .with_display_fields(vec![create_attr_id("name")])
            .build();
        let article = DocumentTypeBuilder::collection("article")
            .with_field(text_field("title"))
            .with_display_fields(vec![create_attr_id("title")])
            .with_relation(RelationDefinition {
                id: create_attr_id("author"),
                relation_type: RelationType::HasOne,
                target: create_doc_type_id("author"),
            })
            .build();

        let registry = SchemaRegistry::try_new(vec![author, article]);
        assert!(registry.is_ok());
    }

    #[test]
    fn test_schema_registry_try_new_invalid_display_fields() {
        let article = DocumentTypeBuilder::collection("article")
            .with_field(text_field("title"))
            .with_display_fields(vec![create_attr_id("missing-field")])
            .build();

        let res = SchemaRegistry::try_new(vec![article]);
        assert!(matches!(
            res,
            Err(DomainError::InvalidSchemaDisplayField { .. })
        ));
    }

    #[test]
    fn test_schema_registry_try_new_unregistered_relation_target() {
        let article = DocumentTypeBuilder::collection("article")
            .with_field(text_field("title"))
            .with_relation(RelationDefinition {
                id: create_attr_id("author"),
                relation_type: RelationType::HasOne,
                target: create_doc_type_id("unregistered-author"),
            })
            .build();

        let res = SchemaRegistry::try_new(vec![article]);
        assert!(
            matches!(res, Err(DomainError::DocumentTypeNotFound(target)) if target.as_ref() == "unregistered-author")
        );
    }

    #[test]
    fn test_schema_registry_try_new_inapplicable_constraint() {
        let invalid_field =
            FieldDefinitionBuilder::new("title", FieldType::Primitive(PrimitiveType::Text))
                .min_int(10)
                .build();

        let article = DocumentTypeBuilder::collection("article")
            .with_field(invalid_field)
            .build();

        let res = SchemaRegistry::try_new(vec![article]);
        assert!(matches!(res, Err(DomainError::Validation(_))));
    }

    #[test]
    fn test_schema_registry_try_new_invalid_decimal_bounds() {
        // precision == 0
        let f_zero_prec = FieldDefinitionBuilder::new(
            "amount",
            FieldType::Primitive(PrimitiveType::Decimal {
                precision: 0,
                scale: 0,
            }),
        )
        .build();
        let dt_zero = DocumentTypeBuilder::collection("item")
            .with_field(f_zero_prec)
            .build();
        assert!(matches!(
            SchemaRegistry::try_new(vec![dt_zero]),
            Err(DomainError::Validation(msg)) if msg.contains("invalid decimal definition")
        ));

        // precision > 38
        let f_large_prec = FieldDefinitionBuilder::new(
            "amount",
            FieldType::Primitive(PrimitiveType::Decimal {
                precision: 39,
                scale: 2,
            }),
        )
        .build();
        let dt_large = DocumentTypeBuilder::collection("item")
            .with_field(f_large_prec)
            .build();
        assert!(matches!(
            SchemaRegistry::try_new(vec![dt_large]),
            Err(DomainError::Validation(msg)) if msg.contains("invalid decimal definition")
        ));

        // scale > precision
        let f_scale_overflow = FieldDefinitionBuilder::new(
            "amount",
            FieldType::Primitive(PrimitiveType::Decimal {
                precision: 5,
                scale: 6,
            }),
        )
        .build();
        let dt_scale = DocumentTypeBuilder::collection("item")
            .with_field(f_scale_overflow)
            .build();
        assert!(matches!(
            SchemaRegistry::try_new(vec![dt_scale]),
            Err(DomainError::Validation(msg)) if msg.contains("invalid decimal definition")
        ));
    }
}
