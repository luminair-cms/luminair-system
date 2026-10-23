//! Schema discovery, JSON parsing, domain mapping, and SystemContext assembly.

use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;

use indexmap::{IndexMap, IndexSet};
use rust_decimal::Decimal;

use domain::common::DisplayName;
use domain::schema::{
    AttributeId, DocumentType, DocumentTypeInfo, DocumentTypeOptions, FieldConstraint,
    FieldDefinition, FieldType, IntegerSize, PrimitiveType, RegexPattern, RelationDefinition,
    SchemaRegistry,
};
use domain::system::{LocaleId, SystemConfig, SystemConfigId, SystemContext};

use super::dto::{
    AttributeRecord, ConstraintRecord, DocumentInfoRecord, DocumentOptionsRecord, DocumentRecord,
    FieldTypeDto, RelationTypeDto, SystemConfigRecord,
};
use super::error::SchemaLoaderError;
use crate::persistence::naming::is_reserved_sql_keyword;

/// Loads schemas and system config from `schema_dir`, validates them,
/// assembles `SystemContext`, and promotes it to `'static` lifetime via `Box::leak`.
pub fn load_system_context_from_dir(
    schema_dir: &Path,
) -> Result<&'static SystemContext, SchemaLoaderError> {
    let (config, registry) = load_schemas_from_dir(schema_dir)?;
    let ctx = SystemContext::new(registry, config);
    Ok(Box::leak(Box::new(ctx)))
}

/// Walks `schema_dir`, loads `system-config.json` and all `document-types/*.json`,
/// validates all cross-schema invariants, and returns `(SystemConfig, SchemaRegistry)`.
pub fn load_schemas_from_dir(
    schema_dir: &Path,
) -> Result<(SystemConfig, SchemaRegistry), SchemaLoaderError> {
    ensure_schema_dir_exists(schema_dir)?;

    let system_config = load_system_config_file(schema_dir)?;
    let doc_types = load_document_types_from_dir(schema_dir)?;
    let registry = SchemaRegistry::try_new(doc_types)?;

    Ok((system_config, registry))
}

/// Parses the system configuration from a `system-config.json` string content (no `id` required).
pub fn parse_system_config(content: &str) -> Result<SystemConfig, SchemaLoaderError> {
    let record: SystemConfigRecord =
        serde_json::from_str(content).map_err(|source| SchemaLoaderError::JsonInline { source })?;

    let default_locale = LocaleId::try_new(record.default_locale).map_err(|e| {
        SchemaLoaderError::Validation(format!(
            "invalid defaultLocale '{}': {e}",
            record.default_locale
        ))
    })?;

    let mut available_locales = Vec::with_capacity(record.available_locales.len());
    for loc in record.available_locales {
        let loc_id = LocaleId::try_new(loc).map_err(|e| {
            SchemaLoaderError::Validation(format!("invalid availableLocale '{loc}': {e}"))
        })?;
        available_locales.push(loc_id);
    }

    let id = SystemConfigId::new(uuid::Uuid::now_v7());
    let config = SystemConfig::new(id, available_locales, default_locale)?;
    Ok(config)
}

/// Parses a single document type definition from JSON string content.
#[allow(clippy::mutable_key_type)]
pub fn parse_document_type(
    content: &str,
    file_stem: &str,
) -> Result<DocumentType, SchemaLoaderError> {
    validate_file_stem(file_stem)?;

    let record: DocumentRecord = serde_json::from_str(content)
        .map_err(|source| SchemaLoaderError::JsonInline { source })?;

    let info = parse_document_info(&record.info, file_stem)?;
    let options = parse_document_options(record.options);
    let (fields, relations) = parse_attributes(record.attributes, file_stem)?;

    Ok(DocumentType {
        id: info.singular_name.clone(),
        kind: record.kind.into(),
        info,
        options,
        fields,
        relations,
    })
}

/// Parses a `FieldTypeDto` into a domain `FieldType`.
pub fn parse_field_type(dto: &FieldTypeDto<'_>) -> Result<FieldType, SchemaLoaderError> {
    match dto {
        FieldTypeDto::Simple(s) => {
            let lower = s.to_ascii_lowercase();
            match lower.as_str() {
                "text" | "string" => Ok(FieldType::Primitive(PrimitiveType::Text)),
                "localizedtext" | "localized_text" => Ok(FieldType::LocalizedText),
                "boolean" | "bool" => Ok(FieldType::Primitive(PrimitiveType::Boolean)),
                "date" => Ok(FieldType::Primitive(PrimitiveType::Date)),
                "datetime" | "date_time" => Ok(FieldType::Primitive(PrimitiveType::DateTime)),
                "email" => Ok(FieldType::Email),
                "url" => Ok(FieldType::Url),
                "json" => Ok(FieldType::Json),
                "uid" => Ok(FieldType::Primitive(PrimitiveType::Uid)),
                "uuid" => Ok(FieldType::Primitive(PrimitiveType::Uuid)),
                "int16" | "short" | "int32" | "integer" | "int64" | "long" => {
                    let size = parse_integer_size(&lower)?;
                    Ok(FieldType::Primitive(PrimitiveType::Integer(size)))
                }
                "decimal" => Ok(FieldType::Primitive(PrimitiveType::Decimal {
                    precision: 10,
                    scale: 2,
                })),
                unknown => Err(SchemaLoaderError::Validation(format!(
                    "unrecognized field type '{unknown}'"
                ))),
            }
        }
        FieldTypeDto::Integer { integer } => {
            let size = parse_integer_size(integer)?;
            Ok(FieldType::Primitive(PrimitiveType::Integer(size)))
        }
        FieldTypeDto::Decimal { decimal } => {
            if decimal.precision == 0
                || decimal.precision > 38
                || decimal.scale > decimal.precision
            {
                return Err(SchemaLoaderError::Validation(format!(
                    "invalid decimal bounds: precision {} (must be in 1..=38) and scale {} (must be <= precision)",
                    decimal.precision, decimal.scale
                )));
            }
            Ok(FieldType::Primitive(PrimitiveType::Decimal {
                precision: decimal.precision,
                scale: decimal.scale,
            }))
        }
    }
}

/// Parses constraint records into domain `FieldConstraint` variants.
#[allow(clippy::mutable_key_type)]
pub fn parse_constraints(
    records: &[ConstraintRecord<'_>],
    field_type: &FieldType,
) -> Result<HashSet<FieldConstraint>, SchemaLoaderError> {
    let mut constraints = HashSet::new();

    for c in records {
        if let Some(min_len) = c.min_length {
            constraints.insert(FieldConstraint::MinLength(min_len));
        }
        if let Some(max_len) = c.max_length {
            constraints.insert(FieldConstraint::MaxLength(max_len));
        }
        if let Some(pat) = c.pattern {
            let pattern = RegexPattern::try_new(pat).map_err(|e| {
                SchemaLoaderError::Validation(format!("invalid regex pattern '{pat}': {e}"))
            })?;
            constraints.insert(FieldConstraint::Pattern(pattern));
        }
        if let Some(ref val) = c.min {
            let (int_val, dec_val) = parse_numeric_bound(val, field_type, "min")?;
            if let Some(i) = int_val {
                constraints.insert(FieldConstraint::MinInteger(i));
            }
            if let Some(d) = dec_val {
                constraints.insert(FieldConstraint::MinDecimal(d));
            }
        }
        if let Some(ref val) = c.max {
            let (int_val, dec_val) = parse_numeric_bound(val, field_type, "max")?;
            if let Some(i) = int_val {
                constraints.insert(FieldConstraint::MaxInteger(i));
            }
            if let Some(d) = dec_val {
                constraints.insert(FieldConstraint::MaxDecimal(d));
            }
        }
    }

    Ok(constraints)
}

/// Parses an integer size identifier into a domain `IntegerSize`.
/// Supported pairs: `int16`/`short`, `int32`/`integer`, `int64`/`long`.
pub fn parse_integer_size(s: &str) -> Result<IntegerSize, SchemaLoaderError> {
    match s.to_ascii_lowercase().as_str() {
        "int16" | "short" => Ok(IntegerSize::I16),
        "int32" | "integer" => Ok(IntegerSize::I32),
        "int64" | "long" => Ok(IntegerSize::I64),
        unknown => Err(SchemaLoaderError::Validation(format!(
            "unrecognized integer size '{unknown}'"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Private Helpers for `load_schemas_from_dir`
// ---------------------------------------------------------------------------

/// Ensures that the schema directory exists on disk.
fn ensure_schema_dir_exists(schema_dir: &Path) -> Result<(), SchemaLoaderError> {
    if !schema_dir.exists() {
        return Err(SchemaLoaderError::Io {
            path: schema_dir.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("schema directory '{}' not found", schema_dir.display()),
            ),
        });
    }
    Ok(())
}

/// Reads and parses `system-config.json` within `schema_dir`.
fn load_system_config_file(schema_dir: &Path) -> Result<SystemConfig, SchemaLoaderError> {
    let config_path = schema_dir.join("system-config.json");
    if !config_path.exists() {
        return Err(SchemaLoaderError::MissingSystemConfig {
            dir: schema_dir.to_path_buf(),
        });
    }

    let config_content = std::fs::read_to_string(&config_path).map_err(|source| {
        SchemaLoaderError::Io {
            path: config_path.clone(),
            source,
        }
    })?;

    parse_system_config(&config_content).map_err(|err| match err {
        SchemaLoaderError::JsonInline { source } => SchemaLoaderError::Json {
            path: config_path,
            source,
        },
        other => other,
    })
}

/// Discovers, reads, and parses all `document-types/*.json` definitions within `schema_dir`.
fn load_document_types_from_dir(schema_dir: &Path) -> Result<Vec<DocumentType>, SchemaLoaderError> {
    let types_dir = schema_dir.join("document-types");
    if !types_dir.is_dir() {
        return Err(SchemaLoaderError::MissingDocumentTypesDir {
            dir: schema_dir.to_path_buf(),
        });
    }

    let entries = std::fs::read_dir(&types_dir).map_err(|source| SchemaLoaderError::Io {
        path: types_dir.clone(),
        source,
    })?;

    let mut entries_vec = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| SchemaLoaderError::Io {
            path: types_dir.clone(),
            source,
        })?;
        entries_vec.push(entry);
    }

    // Sort entries deterministically by file name
    entries_vec.sort_by_key(|e| e.file_name());

    let mut doc_types = Vec::with_capacity(entries_vec.len());
    for entry in entries_vec {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            let doc_type = load_document_type_file(&path)?;
            doc_types.push(doc_type);
        }
    }

    Ok(doc_types)
}

/// Loads and parses a single document type definition file from disk.
fn load_document_type_file(path: &Path) -> Result<DocumentType, SchemaLoaderError> {
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| {
            SchemaLoaderError::Validation(format!(
                "invalid file stem for schema file '{}'",
                path.display()
            ))
        })?;

    let content = std::fs::read_to_string(path).map_err(|source| SchemaLoaderError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    parse_document_type(&content, file_stem).map_err(|err| match err {
        SchemaLoaderError::JsonInline { source } => SchemaLoaderError::Json {
            path: path.to_path_buf(),
            source,
        },
        other => other,
    })
}

// ---------------------------------------------------------------------------
// Private Helpers for `parse_document_type`
// ---------------------------------------------------------------------------

/// Validates that a file stem is not a reserved SQL keyword.
fn validate_file_stem(file_stem: &str) -> Result<(), SchemaLoaderError> {
    if is_reserved_sql_keyword(file_stem) {
        return Err(SchemaLoaderError::ReservedKeyword {
            keyword: file_stem.to_string(),
            context: "document type file stem".into(),
        });
    }
    Ok(())
}

/// Parses and validates `DocumentTypeInfo` from record metadata and file stem.
fn parse_document_info(
    info: &DocumentInfoRecord<'_>,
    file_stem: &str,
) -> Result<DocumentTypeInfo, SchemaLoaderError> {
    if info.singular_name != file_stem {
        return Err(SchemaLoaderError::FileStemMismatch {
            file_stem: file_stem.to_string(),
            singular_name: info.singular_name.to_string(),
        });
    }

    let singular_name = domain::schema::DocumentTypeId::try_new(file_stem).map_err(|e| {
        SchemaLoaderError::Validation(format!("invalid document type id '{file_stem}': {e}"))
    })?;

    if is_reserved_sql_keyword(info.plural_name) {
        return Err(SchemaLoaderError::ReservedKeyword {
            keyword: info.plural_name.to_string(),
            context: "pluralName".into(),
        });
    }

    let plural_name = domain::schema::DocumentTypeId::try_new(info.plural_name).map_err(|e| {
        SchemaLoaderError::Validation(format!("invalid pluralName '{}': {e}", info.plural_name))
    })?;

    let title = DisplayName::try_new(info.display_name).map_err(|e| {
        SchemaLoaderError::Validation(format!("invalid displayName '{}': {e}", info.display_name))
    })?;

    let description = if let Some(desc) = info.description {
        Some(DisplayName::try_new(desc).map_err(|e| {
            SchemaLoaderError::Validation(format!("invalid description '{desc}': {e}"))
        })?)
    } else {
        None
    };

    let mut display_fields = Vec::with_capacity(info.display_fields.len());
    for df in &info.display_fields {
        let attr_id = AttributeId::try_new(*df).map_err(|e| {
            SchemaLoaderError::Validation(format!("invalid displayField '{df}': {e}"))
        })?;
        display_fields.push(attr_id);
    }

    Ok(DocumentTypeInfo {
        title,
        singular_name,
        plural_name,
        description,
        display_fields,
    })
}

/// Maps document options record to domain `DocumentTypeOptions`.
fn parse_document_options(options: Option<DocumentOptionsRecord>) -> DocumentTypeOptions {
    DocumentTypeOptions {
        draft_and_publish: options.map(|o| o.draft_and_publish).unwrap_or(false),
    }
}

/// Parses the attributes map, segregating fields and relations with keyword and duplicate validation.
#[allow(clippy::mutable_key_type)]
fn parse_attributes(
    attributes: IndexMap<&str, AttributeRecord<'_>>,
    file_stem: &str,
) -> Result<(IndexSet<FieldDefinition>, IndexSet<RelationDefinition>), SchemaLoaderError> {
    let mut fields = IndexSet::new();
    let mut relations = IndexSet::new();

    for (attr_name, attr_record) in attributes {
        if is_reserved_sql_keyword(attr_name) {
            return Err(SchemaLoaderError::ReservedKeyword {
                keyword: attr_name.to_string(),
                context: format!("attribute '{attr_name}' on document type '{file_stem}'"),
            });
        }

        let attr_id = AttributeId::try_new(attr_name).map_err(|e| {
            SchemaLoaderError::Validation(format!("invalid attribute name '{attr_name}': {e}"))
        })?;

        match attr_record {
            AttributeRecord::Field {
                field_type: raw_ft,
                unique,
                required,
                constraints: raw_constraints,
            } => {
                let field_def = parse_field_attribute(
                    attr_id.clone(),
                    &raw_ft,
                    unique,
                    required,
                    &raw_constraints,
                )?;
                if !fields.insert(field_def) {
                    return Err(SchemaLoaderError::Validation(format!(
                        "duplicate field attribute '{attr_id}' on document type '{file_stem}'"
                    )));
                }
            }
            AttributeRecord::Relation {
                relation_type,
                target,
            } => {
                let rel_def = parse_relation_attribute(attr_id.clone(), relation_type, target)?;
                if !relations.insert(rel_def) {
                    return Err(SchemaLoaderError::Validation(format!(
                        "duplicate relation attribute '{attr_id}' on document type '{file_stem}'"
                    )));
                }
            }
        }
    }

    Ok((fields, relations))
}

/// Parses a scalar field definition with type and constraints.
#[allow(clippy::mutable_key_type)]
fn parse_field_attribute(
    attr_id: AttributeId,
    raw_ft: &FieldTypeDto<'_>,
    unique: bool,
    required: bool,
    raw_constraints: &[ConstraintRecord<'_>],
) -> Result<FieldDefinition, SchemaLoaderError> {
    let field_type = parse_field_type(raw_ft)?;
    let constraints = parse_constraints(raw_constraints, &field_type)?;
    Ok(FieldDefinition {
        id: attr_id,
        field_type,
        unique,
        required,
        constraints,
    })
}

/// Parses a relation definition with cardinality and target document type identifier.
fn parse_relation_attribute(
    attr_id: AttributeId,
    relation_type: RelationTypeDto,
    target: &str,
) -> Result<RelationDefinition, SchemaLoaderError> {
    let target_id = domain::schema::DocumentTypeId::try_new(target).map_err(|e| {
        SchemaLoaderError::Validation(format!(
            "invalid relation target '{target}' for attribute '{attr_id}': {e}"
        ))
    })?;
    Ok(RelationDefinition {
        id: attr_id,
        relation_type: relation_type.into(),
        target: target_id,
    })
}

// ---------------------------------------------------------------------------
// Private Helpers for `parse_constraints`
// ---------------------------------------------------------------------------

/// Helper to parse a numeric bound from JSON into integer or decimal constraint values.
fn parse_numeric_bound(
    val: &serde_json::Value,
    field_type: &FieldType,
    bound_name: &str,
) -> Result<(Option<i64>, Option<Decimal>), SchemaLoaderError> {
    match val {
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if matches!(field_type, FieldType::Primitive(PrimitiveType::Decimal { .. })) {
                    Ok((None, Some(Decimal::from(i))))
                } else {
                    Ok((Some(i), None))
                }
            } else if let Some(f) = n.as_f64() {
                let dec = Decimal::from_str(&f.to_string()).map_err(|e| {
                    SchemaLoaderError::Validation(format!("invalid {bound_name} number '{f}': {e}"))
                })?;
                Ok((None, Some(dec)))
            } else {
                Ok((None, None))
            }
        }
        serde_json::Value::String(s) => {
            let dec = Decimal::from_str(s).map_err(|e| {
                SchemaLoaderError::Validation(format!("invalid {bound_name} decimal string '{s}': {e}"))
            })?;
            Ok((None, Some(dec)))
        }
        _ => Ok((None, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_system_config_valid() {
        let json = r#"{
            "defaultLocale": "en",
            "availableLocales": ["en", "uk", "fr"]
        }"#;

        let config = parse_system_config(json).expect("valid config");
        assert_eq!(config.default_locale.as_ref(), "en");
        assert!(config.contains_locale(&LocaleId::try_new("uk").unwrap()));
    }

    #[test]
    fn test_parse_system_config_default_not_in_available() {
        let json = r#"{
            "defaultLocale": "es",
            "availableLocales": ["en", "uk"]
        }"#;

        let res = parse_system_config(json);
        assert!(matches!(res, Err(SchemaLoaderError::Domain(_))));
    }

    #[test]
    fn test_parse_document_type_valid() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Blog Article",
                "displayFields": ["title"]
            },
            "options": {
                "draftAndPublish": true
            },
            "attributes": {
                "title": {
                    "type": "text",
                    "required": true,
                    "constraints": [
                        { "minLength": 5 }
                    ]
                },
                "content": {
                    "type": "localizedText",
                    "required": false
                }
            }
        }"#;

        let doc_type = parse_document_type(json, "article").expect("parse document type");
        assert_eq!(doc_type.id.as_ref(), "article");
        assert_eq!(doc_type.kind, domain::schema::DocumentKind::Collection);
        assert!(doc_type.options.draft_and_publish);
        assert_eq!(doc_type.fields.len(), 2);
    }

    #[test]
    fn test_parse_document_type_file_stem_mismatch() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Blog Article"
            },
            "attributes": {}
        }"#;

        let res = parse_document_type(json, "post");
        assert!(matches!(res, Err(SchemaLoaderError::FileStemMismatch { .. })));
    }

    #[test]
    fn test_parse_document_type_reserved_keyword() {
        let json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "user",
                "pluralName": "users",
                "displayName": "User"
            },
            "attributes": {}
        }"#;

        let res = parse_document_type(json, "user");
        assert!(matches!(res, Err(SchemaLoaderError::ReservedKeyword { .. })));
    }

    #[test]
    #[allow(clippy::mutable_key_type)]
    fn test_parse_constraints_with_aliases() {
        let json = r#"[
            { "minLength": 3 },
            { "maxLength": 50 },
            { "regex": "^[a-z]+$" },
            { "minInteger": 10 },
            { "maxInteger": 100 }
        ]"#;

        let records: Vec<ConstraintRecord> = serde_json::from_str(json).unwrap();
        let field_type_text = FieldType::Primitive(PrimitiveType::Text);
        let constraints = parse_constraints(&records, &field_type_text).unwrap();

        assert!(constraints.contains(&FieldConstraint::MinLength(3)));
        assert!(constraints.contains(&FieldConstraint::MaxLength(50)));
        assert!(constraints.contains(&FieldConstraint::Pattern(
            RegexPattern::try_new("^[a-z]+$").unwrap()
        )));
        assert!(constraints.contains(&FieldConstraint::MinInteger(10)));
        assert!(constraints.contains(&FieldConstraint::MaxInteger(100)));

        // Test decimal field with minimum / maximum and minDecimal / maxDecimal aliases
        let json_decimal = r#"[
            { "minDecimal": "9.99" },
            { "maxDecimal": "99.95" }
        ]"#;
        let records_dec: Vec<ConstraintRecord> = serde_json::from_str(json_decimal).unwrap();
        let field_type_dec = FieldType::Primitive(PrimitiveType::Decimal {
            precision: 10,
            scale: 2,
        });
        let constraints_dec = parse_constraints(&records_dec, &field_type_dec).unwrap();

        assert!(constraints_dec.contains(&FieldConstraint::MinDecimal(Decimal::from_str("9.99").unwrap())));
        assert!(constraints_dec.contains(&FieldConstraint::MaxDecimal(Decimal::from_str("99.95").unwrap())));

        // Test min & max with minimum & maximum aliases
        let json_min_max = r#"[
            { "minimum": 5 },
            { "maximum": 25 }
        ]"#;
        let records_min_max: Vec<ConstraintRecord> = serde_json::from_str(json_min_max).unwrap();
        let constraints_int = parse_constraints(&records_min_max, &field_type_text).unwrap();
        assert!(constraints_int.contains(&FieldConstraint::MinInteger(5)));
        assert!(constraints_int.contains(&FieldConstraint::MaxInteger(25)));
    }

    #[test]
    fn test_parse_integer_sizes() {
        assert_eq!(parse_integer_size("int16").unwrap(), IntegerSize::I16);
        assert_eq!(parse_integer_size("short").unwrap(), IntegerSize::I16);
        assert_eq!(parse_integer_size("int32").unwrap(), IntegerSize::I32);
        assert_eq!(parse_integer_size("integer").unwrap(), IntegerSize::I32);
        assert_eq!(parse_integer_size("int64").unwrap(), IntegerSize::I64);
        assert_eq!(parse_integer_size("long").unwrap(), IntegerSize::I64);

        assert!(parse_integer_size("i32").is_err());
        assert!(parse_integer_size("bigint").is_err());

        // Test in FieldTypeDto::Simple
        assert_eq!(
            parse_field_type(&FieldTypeDto::Simple("int16")).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16))
        );
        assert_eq!(
            parse_field_type(&FieldTypeDto::Simple("short")).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16))
        );
        assert_eq!(
            parse_field_type(&FieldTypeDto::Simple("int32")).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32))
        );
        assert_eq!(
            parse_field_type(&FieldTypeDto::Simple("integer")).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32))
        );
        assert_eq!(
            parse_field_type(&FieldTypeDto::Simple("int64")).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64))
        );
        assert_eq!(
            parse_field_type(&FieldTypeDto::Simple("long")).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64))
        );

        // Test in FieldTypeDto::Integer
        assert_eq!(
            parse_field_type(&FieldTypeDto::Integer { integer: "short" }).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16))
        );
        assert_eq!(
            parse_field_type(&FieldTypeDto::Integer { integer: "int32" }).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32))
        );
        assert_eq!(
            parse_field_type(&FieldTypeDto::Integer { integer: "long" }).unwrap(),
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64))
        );
    }
}
