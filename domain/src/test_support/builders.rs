use std::collections::HashSet;

use chrono::{DateTime, Utc};
use indexmap::IndexSet;
use rust_decimal::Decimal;

use super::ids::{create_attr_id, create_doc_type_id, create_user_id};
use crate::auth::profile::UserProfile;
use crate::auth::user::UserId;
use crate::common::{DisplayName, Email};
use crate::schema::attributes::{
    AttributeId, FieldConstraint, FieldDefinition, RelationDefinition,
};
use crate::schema::documents::{
    DocumentKind, DocumentType, DocumentTypeId, DocumentTypeInfo, DocumentTypeOptions,
};
use crate::schema::pattern::RegexPattern;
use crate::schema::types::{FieldType, IntegerSize, PrimitiveType};

/// Fluent builder for constructing `DocumentType` in tests.
pub struct DocumentTypeBuilder {
    id: DocumentTypeId,
    kind: DocumentKind,
    title: String,
    singular: DocumentTypeId,
    plural: DocumentTypeId,
    draft_and_publish: bool,
    display_fields: Vec<AttributeId>,
    fields: IndexSet<FieldDefinition>,
    relations: IndexSet<RelationDefinition>,
}

/// Fluent builder for constructing `FieldDefinition` in tests.
pub struct FieldDefinitionBuilder {
    id: AttributeId,
    field_type: FieldType,
    unique: bool,
    required: bool,
    constraints: HashSet<FieldConstraint>,
}

/// Fluent builder for constructing `UserProfile` in tests.
pub struct UserProfileBuilder {
    user_id: UserId,
    email: Option<Email>,
    name: Option<DisplayName>,
    first_seen_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
}

impl DocumentTypeBuilder {
    /// Creates a collection-kind DocumentType with default singular and plural names.
    pub fn collection(id: &str) -> Self {
        let plural = format!("{id}s");
        Self {
            id: create_doc_type_id(id),
            kind: DocumentKind::Collection,
            title: id.to_string(),
            singular: create_doc_type_id(id),
            plural: create_doc_type_id(&plural),
            draft_and_publish: true,
            display_fields: Vec::new(),
            fields: IndexSet::new(),
            relations: IndexSet::new(),
        }
    }

    /// Creates a singleton DocumentType.
    pub fn singleton(id: &str) -> Self {
        Self {
            id: create_doc_type_id(id),
            kind: DocumentKind::Singleton,
            title: id.to_string(),
            singular: create_doc_type_id(id),
            plural: create_doc_type_id(id),
            draft_and_publish: true,
            display_fields: Vec::new(),
            fields: IndexSet::new(),
            relations: IndexSet::new(),
        }
    }

    /// Creates a single-type DocumentType (alias for `singleton`).
    pub fn single(id: &str) -> Self {
        Self::singleton(id)
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_singular_name(mut self, singular: &str) -> Self {
        self.singular = create_doc_type_id(singular);
        self
    }

    pub fn with_plural_name(mut self, plural: &str) -> Self {
        self.plural = create_doc_type_id(plural);
        self
    }

    pub fn with_draft_and_publish(mut self, draft_and_publish: bool) -> Self {
        self.draft_and_publish = draft_and_publish;
        self
    }

    pub fn with_display_fields(mut self, fields: Vec<AttributeId>) -> Self {
        self.display_fields = fields;
        self
    }

    pub fn with_field(mut self, field: impl Into<FieldDefinition>) -> Self {
        self.fields.insert(field.into());
        self
    }

    pub fn with_relation(mut self, relation: RelationDefinition) -> Self {
        self.relations.insert(relation);
        self
    }

    pub fn build(self) -> DocumentType {
        DocumentType {
            id: self.id,
            kind: self.kind,
            info: DocumentTypeInfo {
                title: DisplayName::try_new(&self.title)
                    .unwrap_or_else(|_| DisplayName::try_new("Test").unwrap()),
                singular_name: self.singular,
                plural_name: self.plural,
                description: None,
                display_fields: self.display_fields,
            },
            options: DocumentTypeOptions {
                draft_and_publish: self.draft_and_publish,
            },
            fields: self.fields,
            relations: self.relations,
        }
    }
}

impl FieldDefinitionBuilder {
    pub fn new(name: &str, field_type: FieldType) -> Self {
        Self {
            id: create_attr_id(name),
            field_type,
            unique: false,
            required: true,
            constraints: HashSet::new(),
        }
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn unique(mut self, unique: bool) -> Self {
        self.unique = unique;
        self
    }

    pub fn min_len(mut self, min: usize) -> Self {
        self.constraints.insert(FieldConstraint::MinLength(min));
        self
    }

    pub fn max_len(mut self, max: usize) -> Self {
        self.constraints.insert(FieldConstraint::MaxLength(max));
        self
    }

    pub fn pattern(mut self, regex_pattern: RegexPattern) -> Self {
        self.constraints
            .insert(FieldConstraint::Pattern(regex_pattern));
        self
    }

    pub fn min_int(mut self, min: i64) -> Self {
        self.constraints.insert(FieldConstraint::MinInteger(min));
        self
    }

    pub fn max_int(mut self, max: i64) -> Self {
        self.constraints.insert(FieldConstraint::MaxInteger(max));
        self
    }

    pub fn min_dec(mut self, min: Decimal) -> Self {
        self.constraints.insert(FieldConstraint::MinDecimal(min));
        self
    }

    pub fn max_dec(mut self, max: Decimal) -> Self {
        self.constraints.insert(FieldConstraint::MaxDecimal(max));
        self
    }

    pub fn build(self) -> FieldDefinition {
        FieldDefinition {
            id: self.id,
            field_type: self.field_type,
            unique: self.unique,
            required: self.required,
            constraints: self.constraints,
        }
    }
}

impl From<FieldDefinitionBuilder> for FieldDefinition {
    fn from(builder: FieldDefinitionBuilder) -> Self {
        builder.build()
    }
}

/// Shorthand builder for text field.
pub fn text_field(name: &str) -> FieldDefinitionBuilder {
    FieldDefinitionBuilder::new(name, FieldType::Primitive(PrimitiveType::Text))
}

/// Shorthand builder for uid field.
pub fn uid_field(name: &str) -> FieldDefinitionBuilder {
    FieldDefinitionBuilder::new(name, FieldType::Primitive(PrimitiveType::Uid))
}

/// Shorthand builder for integer field.
pub fn integer_field(name: &str, size: IntegerSize) -> FieldDefinitionBuilder {
    FieldDefinitionBuilder::new(name, FieldType::Primitive(PrimitiveType::Integer(size)))
}

/// Shorthand builder for decimal field.
pub fn decimal_field(name: &str, precision: u8, scale: u8) -> FieldDefinitionBuilder {
    FieldDefinitionBuilder::new(
        name,
        FieldType::Primitive(PrimitiveType::Decimal { precision, scale }),
    )
}

/// Shorthand builder for localized text field.
pub fn localized_field(name: &str) -> FieldDefinitionBuilder {
    FieldDefinitionBuilder::new(name, FieldType::LocalizedText)
}

// ---------------------------------------------------------------------------
// UserProfileBuilder Implementation
// ---------------------------------------------------------------------------

impl UserProfileBuilder {
    /// Creates a new `UserProfileBuilder` initialized with defaults for `name`.
    #[must_use]
    pub fn new(name: &str) -> Self {
        let now = Utc::now();
        Self {
            user_id: create_user_id(name),
            email: Email::try_new(format!("{name}@example.com")).ok(),
            name: DisplayName::try_new(name).ok(),
            first_seen_at: now,
            last_seen_at: now,
        }
    }

    /// Sets the user id.
    #[must_use]
    pub fn with_user_id(mut self, user_id: UserId) -> Self {
        self.user_id = user_id;
        self
    }

    /// Sets the email address.
    #[must_use]
    pub fn with_email(mut self, email: &str) -> Self {
        self.email = Email::try_new(email).ok();
        self
    }

    /// Clears the email address.
    #[must_use]
    pub fn without_email(mut self) -> Self {
        self.email = None;
        self
    }

    /// Sets the display name.
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = DisplayName::try_new(name).ok();
        self
    }

    /// Clears the display name.
    #[must_use]
    pub fn without_name(mut self) -> Self {
        self.name = None;
        self
    }

    /// Sets the first seen timestamp.
    #[must_use]
    pub fn with_first_seen_at(mut self, dt: DateTime<Utc>) -> Self {
        self.first_seen_at = dt;
        self
    }

    /// Sets the last seen timestamp.
    #[must_use]
    pub fn with_last_seen_at(mut self, dt: DateTime<Utc>) -> Self {
        self.last_seen_at = dt;
        self
    }

    /// Builds the `UserProfile`.
    #[must_use]
    pub fn build(self) -> UserProfile {
        UserProfile::reconstitute(
            self.user_id,
            self.email,
            self.name,
            self.first_seen_at,
            self.last_seen_at,
        )
    }
}

impl From<UserProfileBuilder> for UserProfile {
    fn from(builder: UserProfileBuilder) -> Self {
        builder.build()
    }
}
