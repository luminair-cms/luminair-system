//! Schema discovery, JSON parsing, domain mapping, and SystemContext assembly.

use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;

use indexmap::IndexSet;
use rust_decimal::Decimal;

use domain::common::DisplayName;
use domain::schema::{
    AttributeId, DocumentType, DocumentTypeInfo, DocumentTypeOptions, FieldConstraint,
    FieldDefinition, FieldType, IntegerSize, PrimitiveType, RegexPattern, RelationDefinition,
    SchemaRegistry,
};
use domain::system::{LocaleId, SystemConfig, SystemConfigId, SystemContext};

use super::dto::{
    AttributeRecord, ConstraintRecord, DocumentRecord, FieldTypeDto, SystemConfigRecord,
};
use super::error::SchemaLoaderError;
use super::sanitize::is_reserved_sql_keyword;

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
                "integer" | "int" | "int32" => {
                    Ok(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)))
                }
                "int16" | "short" => {
                    Ok(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)))
                }
                "int64" | "long" | "bigint" => {
                    Ok(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)))
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
            let lower = integer.to_ascii_lowercase();
            match lower.as_str() {
                "int16" | "i16" | "short" => {
                    Ok(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)))
                }
                "int32" | "i32" | "integer" | "int" => {
                    Ok(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)))
                }
                "int64" | "i64" | "long" | "bigint" => {
                    Ok(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)))
                }
                unknown => Err(SchemaLoaderError::Validation(format!(
                    "unrecognized integer size '{unknown}'"
                ))),
            }
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
        if let Some(min_len) = c.min_length.or(c.min_len_alias) {
            constraints.insert(FieldConstraint::MinLength(min_len));
        }
        if let Some(max_len) = c.max_length.or(c.max_len_alias) {
            constraints.insert(FieldConstraint::MaxLength(max_len));
        }
        if let Some(pat) = c.pattern.or(c.regex) {
            let pattern = RegexPattern::try_new(pat).map_err(|e| {
                SchemaLoaderError::Validation(format!("invalid regex pattern '{pat}': {e}"))
            })?;
            constraints.insert(FieldConstraint::Pattern(pattern));
        }
        if let Some(min_int) = c.min_integer {
            constraints.insert(FieldConstraint::MinInteger(min_int));
        }
        if let Some(max_int) = c.max_integer {
            constraints.insert(FieldConstraint::MaxInteger(max_int));
        }
        if let Some(min_dec_str) = c.min_decimal {
            let dec = Decimal::from_str(min_dec_str).map_err(|e| {
                SchemaLoaderError::Validation(format!("invalid minDecimal '{min_dec_str}': {e}"))
            })?;
            constraints.insert(FieldConstraint::MinDecimal(dec));
        }
        if let Some(max_dec_str) = c.max_decimal {
            let dec = Decimal::from_str(max_dec_str).map_err(|e| {
                SchemaLoaderError::Validation(format!("invalid maxDecimal '{max_dec_str}': {e}"))
            })?;
            constraints.insert(FieldConstraint::MaxDecimal(dec));
        }
        // Generic min / max
        if let Some(ref val) = c.min {
            match val {
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        if matches!(field_type, FieldType::Primitive(PrimitiveType::Decimal { .. }))
                        {
                            constraints.insert(FieldConstraint::MinDecimal(Decimal::from(i)));
                        } else {
                            constraints.insert(FieldConstraint::MinInteger(i));
                        }
                    } else if let Some(f) = n.as_f64() {
                        let dec = Decimal::from_str(&f.to_string()).map_err(|e| {
                            SchemaLoaderError::Validation(format!("invalid min number '{f}': {e}"))
                        })?;
                        constraints.insert(FieldConstraint::MinDecimal(dec));
                    }
                }
                serde_json::Value::String(s) => {
                    let dec = Decimal::from_str(s).map_err(|e| {
                        SchemaLoaderError::Validation(format!(
                            "invalid min decimal string '{s}': {e}"
                        ))
                    })?;
                    constraints.insert(FieldConstraint::MinDecimal(dec));
                }
                _ => {}
            }
        }
        if let Some(ref val) = c.max {
            match val {
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        if matches!(field_type, FieldType::Primitive(PrimitiveType::Decimal { .. }))
                        {
                            constraints.insert(FieldConstraint::MaxDecimal(Decimal::from(i)));
                        } else {
                            constraints.insert(FieldConstraint::MaxInteger(i));
                        }
                    } else if let Some(f) = n.as_f64() {
                        let dec = Decimal::from_str(&f.to_string()).map_err(|e| {
                            SchemaLoaderError::Validation(format!("invalid max number '{f}': {e}"))
                        })?;
                        constraints.insert(FieldConstraint::MaxDecimal(dec));
                    }
                }
                serde_json::Value::String(s) => {
                    let dec = Decimal::from_str(s).map_err(|e| {
                        SchemaLoaderError::Validation(format!(
                            "invalid max decimal string '{s}': {e}"
                        ))
                    })?;
                    constraints.insert(FieldConstraint::MaxDecimal(dec));
                }
                _ => {}
            }
        }
    }

    Ok(constraints)
}

/// Parses a single document type definition from JSON string content.
#[allow(clippy::mutable_key_type)]
pub fn parse_document_type(
    content: &str,
    file_stem: &str,
) -> Result<DocumentType, SchemaLoaderError> {
    if is_reserved_sql_keyword(file_stem) {
        return Err(SchemaLoaderError::ReservedKeyword {
            keyword: file_stem.to_string(),
            context: "document type file stem".into(),
        });
    }

    let record: DocumentRecord =
        serde_json::from_str(content).map_err(|e| SchemaLoaderError::JsonInline { source: e })?;

    if record.info.singular_name != file_stem {
        return Err(SchemaLoaderError::FileStemMismatch {
            file_stem: file_stem.to_string(),
            singular_name: record.info.singular_name.to_string(),
        });
    }

    let id = domain::schema::DocumentTypeId::try_new(file_stem).map_err(|e| {
        SchemaLoaderError::Validation(format!("invalid document type id '{file_stem}': {e}"))
    })?;

    if is_reserved_sql_keyword(record.info.plural_name) {
        return Err(SchemaLoaderError::ReservedKeyword {
            keyword: record.info.plural_name.to_string(),
            context: "pluralName".into(),
        });
    }

    let plural_name =
        domain::schema::DocumentTypeId::try_new(record.info.plural_name).map_err(|e| {
            SchemaLoaderError::Validation(format!(
                "invalid pluralName '{}': {e}",
                record.info.plural_name
            ))
        })?;

    let title = DisplayName::try_new(record.info.display_name).map_err(|e| {
        SchemaLoaderError::Validation(format!(
            "invalid displayName '{}': {e}",
            record.info.display_name
        ))
    })?;

    let description = if let Some(desc) = record.info.description {
        Some(DisplayName::try_new(desc).map_err(|e| {
            SchemaLoaderError::Validation(format!("invalid description '{desc}': {e}"))
        })?)
    } else {
        None
    };

    let mut display_fields = Vec::with_capacity(record.info.display_fields.len());
    for df in &record.info.display_fields {
        let attr_id = AttributeId::try_new(*df).map_err(|e| {
            SchemaLoaderError::Validation(format!("invalid displayField '{df}': {e}"))
        })?;
        display_fields.push(attr_id);
    }

    let info = DocumentTypeInfo {
        title,
        singular_name: id.clone(),
        plural_name,
        description,
        display_fields,
    };

    let options = DocumentTypeOptions {
        draft_and_publish: record.options.map(|o| o.draft_and_publish).unwrap_or(false),
    };

    let mut fields = IndexSet::new();
    let mut relations = IndexSet::new();

    for (attr_name, attr_record) in record.attributes {
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
                let ft = parse_field_type(&raw_ft)?;
                let constraints = parse_constraints(&raw_constraints, &ft)?;
                let field_def = FieldDefinition {
                    id: attr_id.clone(),
                    field_type: ft,
                    unique,
                    required,
                    constraints,
                };
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
                let target_id =
                    domain::schema::DocumentTypeId::try_new(target).map_err(|e| {
                        SchemaLoaderError::Validation(format!(
                            "invalid relation target '{target}' for attribute '{attr_id}': {e}"
                        ))
                    })?;
                let rel_def = RelationDefinition {
                    id: attr_id.clone(),
                    relation_type: relation_type.into(),
                    target: target_id,
                };
                if !relations.insert(rel_def) {
                    return Err(SchemaLoaderError::Validation(format!(
                        "duplicate relation attribute '{attr_id}' on document type '{file_stem}'"
                    )));
                }
            }
        }
    }

    Ok(DocumentType {
        id,
        kind: record.kind.into(),
        info,
        options,
        fields,
        relations,
    })
}

/// Parses the system configuration from a `system-config.json` string content (no `id` required).
pub fn parse_system_config(content: &str) -> Result<SystemConfig, SchemaLoaderError> {
    let record: SystemConfigRecord =
        serde_json::from_str(content).map_err(|e| SchemaLoaderError::JsonInline { source: e })?;

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

/// Walks `schema_dir`, loads `system-config.json` and all `document-types/*.json`,
/// validates all cross-schema invariants, and returns `(SystemConfig, SchemaRegistry)`.
pub fn load_schemas_from_dir(
    schema_dir: &Path,
) -> Result<(SystemConfig, SchemaRegistry), SchemaLoaderError> {
    if !schema_dir.exists() {
        return Err(SchemaLoaderError::Io {
            path: schema_dir.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("schema directory '{}' not found", schema_dir.display()),
            ),
        });
    }

    // 1. Read and parse system-config.json
    let config_path = schema_dir.join("system-config.json");
    if !config_path.exists() {
        return Err(SchemaLoaderError::MissingSystemConfig {
            dir: schema_dir.to_path_buf(),
        });
    }

    let config_content = std::fs::read_to_string(&config_path).map_err(|e| {
        SchemaLoaderError::Io {
            path: config_path.clone(),
            source: e,
        }
    })?;

    let system_config = parse_system_config(&config_content).map_err(|e| match e {
        SchemaLoaderError::JsonInline { source } => SchemaLoaderError::Json {
            path: config_path.clone(),
            source,
        },
        other => other,
    })?;

    // 2. Discover document-types/
    let types_dir = schema_dir.join("document-types");
    if !types_dir.is_dir() {
        return Err(SchemaLoaderError::MissingDocumentTypesDir {
            dir: schema_dir.to_path_buf(),
        });
    }

    let entries = std::fs::read_dir(&types_dir).map_err(|e| SchemaLoaderError::Io {
        path: types_dir.clone(),
        source: e,
    })?;

    let mut doc_types = Vec::new();
    let mut entries_vec = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| SchemaLoaderError::Io {
            path: types_dir.clone(),
            source: e,
        })?;
        entries_vec.push(entry);
    }

    // Sort entries deterministically by file name
    entries_vec.sort_by_key(|e| e.file_name());

    for entry in entries_vec {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    SchemaLoaderError::Validation(format!(
                        "invalid file stem for schema file '{}'",
                        path.display()
                    ))
                })?;

            let content = std::fs::read_to_string(&path).map_err(|e| {
                SchemaLoaderError::Io {
                    path: path.clone(),
                    source: e,
                }
            })?;

            let doc_type = parse_document_type(&content, file_stem).map_err(|e| match e {
                SchemaLoaderError::JsonInline { source } => SchemaLoaderError::Json {
                    path: path.clone(),
                    source,
                },
                other => other,
            })?;

            doc_types.push(doc_type);
        }
    }

    // 3. Construct and validate SchemaRegistry
    let registry = SchemaRegistry::try_new(doc_types)?;

    Ok((system_config, registry))
}

/// Loads schemas and system config from `schema_dir`, validates them,
/// assembles `SystemContext`, and promotes it to `'static` lifetime via `Box::leak`.
pub fn load_system_context_from_dir(
    schema_dir: &Path,
) -> Result<&'static SystemContext, SchemaLoaderError> {
    let (config, registry) = load_schemas_from_dir(schema_dir)?;
    let ctx = SystemContext::new(registry, config);
    Ok(Box::leak(Box::new(ctx)))
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
}
