use std::{
    borrow::Borrow,
    collections::HashSet,
    hash::{Hash, Hasher},
};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::schema::{AttributeId, DocumentTypeId, FieldType, PrimitiveType};

#[derive(Clone, Debug, Serialize)]
pub struct FieldDefinition {
    pub id: AttributeId,
    pub field_type: FieldType,
    pub unique: bool,
    pub required: bool,
    pub constraints: HashSet<FieldConstraint>,
}

/// A uniquely identifiable document Relation.
#[derive(Clone, Debug, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldConstraint {
    Pattern(String),
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