//! Zero-copy borrowed Serde records for schema JSON definitions.
//!
//! All DTOs borrow string slices directly from the file buffer (`&'a str`), eliminating
//! intermediate heap allocations during parsing before conversion to domain types.

use indexmap::IndexMap;
use serde::Deserialize;

/// Top-level record for a document type JSON file.
#[derive(Clone, Debug, Deserialize)]
#[serde(bound = "'de: 'a")]
#[serde(rename_all = "camelCase")]
pub struct DocumentRecord<'a> {
    #[serde(alias = "type")]
    pub kind: DocumentKindDto,
    pub info: DocumentInfoRecord<'a>,
    #[serde(default)]
    pub options: Option<DocumentOptionsRecord>,
    pub attributes: IndexMap<&'a str, AttributeRecord<'a>>,
}

/// Document kind (collection vs singleton).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentKindDto {
    Collection,
    #[serde(alias = "singleType")]
    Singleton,
}

/// Metadata describing the document type and composite display fields.
#[derive(Clone, Debug, Deserialize)]
#[serde(bound = "'de: 'a")]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfoRecord<'a> {
    #[serde(alias = "title")]
    pub display_name: &'a str,
    pub singular_name: &'a str,
    pub plural_name: &'a str,
    #[serde(default)]
    pub description: Option<&'a str>,
    #[serde(default)]
    pub display_fields: Vec<&'a str>,
}

/// Lifecycle options for the document type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DocumentOptionsRecord {
    #[serde(default)]
    pub draft_and_publish: bool,
}

/// Untagged attribute record discriminating relational associations from typed scalar fields.
#[derive(Clone, Debug, Deserialize)]
#[serde(bound = "'de: 'a")]
#[serde(rename_all = "camelCase", untagged)]
pub enum AttributeRecord<'a> {
    /// Relational association targeting another document type.
    Relation {
        #[serde(alias = "relation")]
        relation_type: RelationTypeDto,
        target: &'a str,
    },
    /// Typed scalar field with validation constraints.
    Field {
        #[serde(alias = "type")]
        field_type: FieldTypeDto<'a>,
        #[serde(default)]
        unique: bool,
        #[serde(default)]
        required: bool,
        #[serde(default)]
        constraints: Vec<ConstraintRecord<'a>>,
    },
}

/// Permissive scalar field type descriptor supporting simple strings and structured bounds.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(bound = "'de: 'a")]
#[serde(untagged)]
pub enum FieldTypeDto<'a> {
    /// String shorthand (e.g. "text", "localizedText", "boolean", "date", "dateTime", "email", "url", "json", "uid", "uuid", "int32", "integer").
    Simple(&'a str),
    /// Structured integer (e.g. {"integer": "int16" | "int32" | "int64" | "short" | "integer" | "long"}).
    Integer { integer: &'a str },
    /// Structured decimal with precision and scale (e.g. {"decimal": {"precision": 10, "scale": 2}}).
    Decimal { decimal: DecimalBoundsDto },
}

/// Decimal precision and scale bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct DecimalBoundsDto {
    pub precision: u8,
    pub scale: u8,
}

/// Relation cardinality and direction matching domain `RelationType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RelationTypeDto {
    HasOne,
    HasMany,
    BelongsToOne,
    BelongsToMany,
}

/// Attribute constraint DTO supporting standard keys and aliases.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(bound = "'de: 'a")]
#[serde(rename_all = "camelCase")]
pub struct ConstraintRecord<'a> {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    #[serde(alias = "minimum", alias = "minInteger", alias = "minDecimal")]
    pub min: Option<serde_json::Value>,
    #[serde(alias = "maximum", alias = "maxInteger", alias = "maxDecimal")]
    pub max: Option<serde_json::Value>,
    #[serde(alias = "regex")]
    pub pattern: Option<&'a str>,
}

/// Root DTO for `system-config.json` (no `id` required).
#[derive(Clone, Debug, Deserialize)]
#[serde(bound = "'de: 'a")]
#[serde(rename_all = "camelCase")]
pub struct SystemConfigRecord<'a> {
    pub default_locale: &'a str,
    pub available_locales: Vec<&'a str>,
    #[serde(default)]
    pub instance_name: Option<&'a str>,
}

impl From<DocumentKindDto> for domain::schema::DocumentKind {
    fn from(dto: DocumentKindDto) -> Self {
        match dto {
            DocumentKindDto::Collection => domain::schema::DocumentKind::Collection,
            DocumentKindDto::Singleton => domain::schema::DocumentKind::Singleton,
        }
    }
}

impl From<RelationTypeDto> for domain::schema::RelationType {
    fn from(dto: RelationTypeDto) -> Self {
        match dto {
            RelationTypeDto::HasOne => domain::schema::RelationType::HasOne,
            RelationTypeDto::HasMany => domain::schema::RelationType::HasMany,
            RelationTypeDto::BelongsToOne => domain::schema::RelationType::BelongsToOne,
            RelationTypeDto::BelongsToMany => domain::schema::RelationType::BelongsToMany,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_document_record_with_fields_and_relations() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Blog Article",
                "description": "Company articles",
                "displayFields": ["title", "category"]
            },
            "options": {
                "draftAndPublish": true
            },
            "attributes": {
                "title": {
                    "type": "text",
                    "required": true,
                    "constraints": [
                        { "minLength": 5 },
                        { "maxLength": 100 }
                    ]
                },
                "summary": {
                    "type": "localizedText",
                    "required": true,
                    "constraints": [
                        { "minLength": 10 }
                    ]
                },
                "priority": {
                    "type": { "integer": "int32" },
                    "required": true,
                    "unique": true,
                    "constraints": [
                        { "min": 1 },
                        { "max": 10 }
                    ]
                },
                "price": {
                    "type": { "decimal": { "precision": 10, "scale": 2 } },
                    "required": false
                },
                "category": {
                    "relation": "hasOne",
                    "target": "category"
                },
                "author": {
                    "relation": "belongsToOne",
                    "target": "author"
                },
                "tags": {
                    "relation": "hasMany",
                    "target": "tag"
                }
            }
        }"#;

        let record: DocumentRecord =
            serde_json::from_str(json).expect("deserialize DocumentRecord");
        assert_eq!(record.kind, DocumentKindDto::Collection);
        assert_eq!(record.info.singular_name, "article");
        assert_eq!(record.info.plural_name, "articles");
        assert_eq!(record.info.display_name, "Blog Article");
        assert_eq!(record.info.display_fields, vec!["title", "category"]);
        assert!(record.options.unwrap().draft_and_publish);

        assert_eq!(record.attributes.len(), 7);

        // Field check
        let title = record.attributes.get("title").expect("title attribute");
        match title {
            AttributeRecord::Field {
                field_type,
                required,
                unique,
                constraints,
            } => {
                assert_eq!(*field_type, FieldTypeDto::Simple("text"));
                assert!(*required);
                assert!(!*unique);
                assert_eq!(constraints.len(), 2);
                assert_eq!(constraints[0].min_length, Some(5));
                assert_eq!(constraints[1].max_length, Some(100));
            }
            _ => panic!("expected field"),
        }

        // LocalizedText check
        let summary = record.attributes.get("summary").expect("summary attribute");
        match summary {
            AttributeRecord::Field {
                field_type,
                constraints,
                ..
            } => {
                assert_eq!(*field_type, FieldTypeDto::Simple("localizedText"));
                assert_eq!(constraints[0].min_length, Some(10));
            }
            _ => panic!("expected field"),
        }

        // Relation check
        let category = record
            .attributes
            .get("category")
            .expect("category attribute");
        match category {
            AttributeRecord::Relation {
                relation_type,
                target,
            } => {
                assert_eq!(*relation_type, RelationTypeDto::HasOne);
                assert_eq!(*target, "category");
            }
            _ => panic!("expected relation"),
        }

        let author = record.attributes.get("author").expect("author attribute");
        match author {
            AttributeRecord::Relation {
                relation_type,
                target,
            } => {
                assert_eq!(*relation_type, RelationTypeDto::BelongsToOne);
                assert_eq!(*target, "author");
            }
            _ => panic!("expected relation"),
        }
    }

    #[test]
    fn test_deserialize_system_config_record() {
        let json = r#"{
            "defaultLocale": "en",
            "availableLocales": ["en", "uk", "fr", "de"],
            "instanceName": "Luminair Production"
        }"#;

        let record: SystemConfigRecord =
            serde_json::from_str(json).expect("deserialize SystemConfigRecord");
        assert_eq!(record.default_locale, "en");
        assert_eq!(record.available_locales, vec!["en", "uk", "fr", "de"]);
        assert_eq!(record.instance_name, Some("Luminair Production"));
    }

    #[test]
    fn test_deserialize_document_kind_singleton_and_aliases() {
        #[derive(Deserialize)]
        struct KindWrapper {
            kind: DocumentKindDto,
        }

        let json_singleton = r#"{ "kind": "singleton" }"#;
        let w: KindWrapper = serde_json::from_str(json_singleton).unwrap();
        assert_eq!(w.kind, DocumentKindDto::Singleton);
        assert_eq!(
            domain::schema::DocumentKind::from(w.kind),
            domain::schema::DocumentKind::Singleton
        );

        let json_single_type = r#"{ "kind": "singleType" }"#;
        let w2: KindWrapper = serde_json::from_str(json_single_type).unwrap();
        assert_eq!(w2.kind, DocumentKindDto::Singleton);

        let json_collection = r#"{ "kind": "collection" }"#;
        let w3: KindWrapper = serde_json::from_str(json_collection).unwrap();
        assert_eq!(w3.kind, DocumentKindDto::Collection);
        assert_eq!(
            domain::schema::DocumentKind::from(w3.kind),
            domain::schema::DocumentKind::Collection
        );
    }
}
