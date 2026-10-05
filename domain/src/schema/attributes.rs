use std::{
    borrow::Borrow,
    collections::HashSet,
    hash::{Hash, Hasher},
};

use nutype::nutype;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::schema::documents::DocumentTypeId;
use crate::schema::pattern::RegexPattern;
use crate::schema::types::{FieldType, PrimitiveType};

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
pub struct AttributeId(String);

#[derive(Clone, Debug)]
pub struct FieldDefinition {
    pub id: AttributeId,
    pub field_type: FieldType,
    pub unique: bool,
    pub required: bool,
    pub constraints: HashSet<FieldConstraint>,
}

/// A uniquely identifiable document Relation.
#[derive(Clone, Debug)]
pub struct RelationDefinition {
    pub id: AttributeId,
    pub relation_type: RelationType,
    pub target: DocumentTypeId,
}

impl PartialEq for FieldDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for FieldDefinition {}

impl Hash for FieldDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.as_ref().hash(state);
    }
}

impl Borrow<AttributeId> for FieldDefinition {
    fn borrow(&self) -> &AttributeId {
        &self.id
    }
}

impl PartialEq for RelationDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for RelationDefinition {}

impl Hash for RelationDefinition {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Borrow<AttributeId> for RelationDefinition {
    fn borrow(&self) -> &AttributeId {
        &self.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldConstraint {
    Pattern(RegexPattern),
    MinLength(usize),
    MaxLength(usize),
    MinInteger(i64),
    MaxInteger(i64),
    MinDecimal(Decimal),
    MaxDecimal(Decimal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelationType {
    // owning side
    HasOne,
    HasMany,
    // inverse side
    BelongsToOne,
    BelongsToMany,
}

impl FieldConstraint {
    pub fn is_applicable_for(&self, ft: &FieldType) -> bool {
        match self {
            FieldConstraint::Pattern(_) => matches!(
                ft,
                FieldType::Primitive(PrimitiveType::Text | PrimitiveType::Uid)
                    | FieldType::LocalizedText
            ),
            FieldConstraint::MinLength(_) | FieldConstraint::MaxLength(_) => matches!(
                ft,
                FieldType::Primitive(PrimitiveType::Text | PrimitiveType::Uid)
                    | FieldType::LocalizedText
            ),
            FieldConstraint::MinInteger(_) | FieldConstraint::MaxInteger(_) => {
                matches!(ft, FieldType::Primitive(PrimitiveType::Integer(_)))
            }
            FieldConstraint::MinDecimal(_) | FieldConstraint::MaxDecimal(_) => {
                matches!(ft, FieldType::Primitive(PrimitiveType::Decimal { .. }))
            }
        }
    }
}

impl RelationType {
    pub fn is_owning(&self) -> bool {
        matches!(self, RelationType::HasOne | RelationType::HasMany)
    }
    pub fn is_inverse(&self) -> bool {
        matches!(
            self,
            RelationType::BelongsToOne | RelationType::BelongsToMany
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_id_valid_kebab_case() {
        assert!(AttributeId::try_new("title").is_ok());
        assert!(AttributeId::try_new("body-text").is_ok());
        assert!(AttributeId::try_new("field-1").is_ok());
    }

    #[test]
    fn test_attribute_id_rejects_underscores_and_uppercase() {
        assert!(AttributeId::try_new("body_text").is_err());
        assert!(AttributeId::try_new("Title").is_err());
        assert!(AttributeId::try_new("myField").is_err());
    }

    #[test]
    fn test_attribute_id_starts_with_digit_or_hyphen() {
        assert!(AttributeId::try_new("1title").is_err());
        assert!(AttributeId::try_new("-private").is_err());
        assert!(AttributeId::try_new("_private").is_err());
    }

    #[test]
    fn test_field_constraint_applicability() {
        let pattern = FieldConstraint::Pattern(RegexPattern::try_new(r"^[a-z]+$").unwrap());
        assert!(pattern.is_applicable_for(&FieldType::Primitive(PrimitiveType::Text)));
        assert!(pattern.is_applicable_for(&FieldType::Primitive(PrimitiveType::Uid)));
        assert!(pattern.is_applicable_for(&FieldType::LocalizedText));
        assert!(!pattern.is_applicable_for(&FieldType::Primitive(PrimitiveType::Boolean)));

        let min_int = FieldConstraint::MinInteger(0);
        assert!(min_int.is_applicable_for(&FieldType::Primitive(PrimitiveType::Integer(
            crate::schema::types::IntegerSize::I32
        ))));
        assert!(!min_int.is_applicable_for(&FieldType::Primitive(PrimitiveType::Text)));
    }

    #[test]
    fn test_relation_type_owning_and_inverse() {
        assert!(RelationType::HasOne.is_owning());
        assert!(RelationType::HasMany.is_owning());
        assert!(!RelationType::HasOne.is_inverse());
        assert!(!RelationType::HasMany.is_inverse());

        assert!(RelationType::BelongsToOne.is_inverse());
        assert!(RelationType::BelongsToMany.is_inverse());
        assert!(!RelationType::BelongsToOne.is_owning());
        assert!(!RelationType::BelongsToMany.is_owning());
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn test_field_and_relation_definition_borrow_lookup() {
        let title_attr = AttributeId::try_new("title").unwrap();
        let author_attr = AttributeId::try_new("author").unwrap();
        let doc_type_id = crate::schema::documents::DocumentTypeId::try_new("user").unwrap();

        let field_def = FieldDefinition {
            id: title_attr.clone(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            unique: false,
            required: true,
            constraints: HashSet::new(),
        };

        let rel_def = RelationDefinition {
            id: author_attr.clone(),
            relation_type: RelationType::HasOne,
            target: doc_type_id,
        };

        let mut field_set = HashSet::new();
        field_set.insert(field_def);
        // Borrow<AttributeId> enables &AttributeId lookup in HashSet<FieldDefinition>
        assert!(field_set.contains(&title_attr));
        assert!(!field_set.contains(&author_attr));

        let mut rel_set = HashSet::new();
        rel_set.insert(rel_def);
        assert!(rel_set.contains(&author_attr));
        assert!(!rel_set.contains(&title_attr));
    }
}