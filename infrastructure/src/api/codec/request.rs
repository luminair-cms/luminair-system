//! Transport-agnostic request payload decoding into domain content and relation mutations.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use application::commands::RelationAction;
use application::errors::{ApplicationError, InvalidParam};
use domain::common::{Email, Url};
use domain::content::instance::DocumentInstanceId;
use domain::content::values::{ContentValue, DomainValue, PrimitiveValue};
use domain::schema::attributes::AttributeId;
use domain::schema::documents::DocumentType;
use domain::schema::types::{FieldType, IntegerSize, PrimitiveType};
use domain::system::LocaleId;

// ---------------------------------------------------------------------------
// Types First (Newspaper Style)
// ---------------------------------------------------------------------------

/// Decoded document payload containing field values and relation actions.
pub type DecodedPayload = (
    HashMap<AttributeId, ContentValue>,
    HashMap<AttributeId, RelationAction>,
);

// ---------------------------------------------------------------------------
// Public Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Decodes an owned JSON object map into domain [`ContentValue`]s driven by schema definitions.
///
/// Accumulates all field errors without truncating at the first failure.
pub fn decode_fields_map(
    doc_type: &DocumentType,
    raw: serde_json::Map<String, serde_json::Value>,
) -> Result<HashMap<AttributeId, ContentValue>, Vec<InvalidParam>> {
    let mut fields = HashMap::with_capacity(raw.len());
    let mut errors = Vec::new();

    for (key, val) in raw {
        decode_single_field(doc_type, key, val, &mut fields, &mut errors);
    }

    if errors.is_empty() {
        Ok(fields)
    } else {
        Err(errors)
    }
}

/// Decodes an owned JSON object map into relation mutation actions driven by schema definitions.
///
/// Supports both shorthand arrays (`{"tags": ["uuid1", "uuid2"]}`) and explicit action objects
/// (`{"tags": {"connect": ["uuid1"]}}`, `{"tags": {"unset": true}}`).
pub fn decode_relations_map(
    doc_type: &DocumentType,
    raw: serde_json::Map<String, serde_json::Value>,
) -> Result<HashMap<AttributeId, RelationAction>, Vec<InvalidParam>> {
    let mut relations = HashMap::with_capacity(raw.len());
    let mut errors = Vec::new();

    for (key, val) in raw {
        decode_single_relation(doc_type, key, val, &mut relations, &mut errors);
    }

    if errors.is_empty() {
        Ok(relations)
    } else {
        Err(errors)
    }
}

/// Decodes both fields and optional relations maps from a request payload into domain mutations.
///
/// Gathers all field and relational validation errors into an [`ApplicationError::Validation`].
pub fn decode_document_payload(
    doc_type: &DocumentType,
    fields_raw: serde_json::Map<String, serde_json::Value>,
    relations_raw: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<DecodedPayload, ApplicationError> {
    let mut all_errors = Vec::new();

    let fields = match decode_fields_map(doc_type, fields_raw) {
        Ok(f) => Some(f),
        Err(errs) => {
            all_errors.extend(errs);
            None
        }
    };

    let relations = match relations_raw {
        Some(raw) => match decode_relations_map(doc_type, raw) {
            Ok(r) => Some(r),
            Err(errs) => {
                all_errors.extend(errs);
                None
            }
        },
        None => Some(HashMap::new()),
    };

    if !all_errors.is_empty() {
        return Err(ApplicationError::Validation(all_errors));
    }

    match (fields, relations) {
        (Some(f), Some(r)) => Ok((f, r)),
        _ => Err(ApplicationError::Validation(all_errors)),
    }
}

// ---------------------------------------------------------------------------
// Private Helpers - Fields
// ---------------------------------------------------------------------------

fn decode_single_field(
    doc_type: &DocumentType,
    raw_key: String,
    val: serde_json::Value,
    fields: &mut HashMap<AttributeId, ContentValue>,
    errors: &mut Vec<InvalidParam>,
) {
    let attr_id = match AttributeId::try_new(raw_key.clone()) {
        Ok(id) => id,
        Err(_) => {
            errors.push(InvalidParam::new(
                raw_key,
                "invalid attribute name format",
                "invalid_attribute_name",
            ));
            return;
        }
    };

    let Some(field_def) = doc_type.fields.get(&attr_id) else {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!("attribute '{attr_id}' is not declared in schema"),
            "unknown_attribute",
        ));
        return;
    };

    if val.is_null() {
        fields.insert(attr_id, ContentValue::Null);
        return;
    }

    if let Some(content_val) = decode_field_value(val, field_def.field_type, &attr_id, errors) {
        fields.insert(attr_id, content_val);
    }
}

fn decode_field_value(
    val: serde_json::Value,
    field_type: FieldType,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<ContentValue> {
    match field_type {
        FieldType::LocalizedText => decode_localized_text(val, attr_id, errors),
        FieldType::Email => decode_email(val, attr_id, errors),
        FieldType::Url => decode_url(val, attr_id, errors),
        FieldType::Json => decode_json_map(val, attr_id, errors),
        FieldType::Primitive(pt) => decode_primitive(val, pt, attr_id, errors)
            .map(|p| ContentValue::Scalar(DomainValue::Primitive(p))),
    }
}

fn decode_localized_text(
    val: serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<ContentValue> {
    let map = match val {
        serde_json::Value::Object(map) => map,
        _ => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("expected JSON object for localized text field '{attr_id}'"),
                "invalid_field_value",
            ));
            return None;
        }
    };

    let mut localized_map = HashMap::with_capacity(map.len());
    let mut field_had_error = false;

    for (loc_key, text_val) in map {
        let loc = match LocaleId::try_new(loc_key.clone()) {
            Ok(l) => l,
            Err(_) => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("invalid locale '{loc_key}' in field '{attr_id}'"),
                    "unknown_locale",
                ));
                field_had_error = true;
                continue;
            }
        };

        match text_val {
            serde_json::Value::String(s) => {
                localized_map.insert(loc, s);
            }
            _ => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("expected string value for locale '{loc}' in field '{attr_id}'"),
                    "invalid_field_value",
                ));
                field_had_error = true;
            }
        }
    }

    if field_had_error {
        None
    } else {
        Some(ContentValue::LocalizedText(localized_map))
    }
}

fn decode_email(
    val: serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<ContentValue> {
    match val {
        serde_json::Value::String(s) => match Email::try_new(s) {
            Ok(email) => Some(ContentValue::Scalar(DomainValue::Email(email))),
            Err(e) => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("invalid email format for '{attr_id}': {e}"),
                    "invalid_field_value",
                ));
                None
            }
        },
        _ => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("expected string email for '{attr_id}'"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn decode_url(
    val: serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<ContentValue> {
    match val {
        serde_json::Value::String(s) => match Url::try_new(s) {
            Ok(url) => Some(ContentValue::Scalar(DomainValue::Url(url))),
            Err(e) => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("invalid URL format for '{attr_id}': {e}"),
                    "invalid_field_value",
                ));
                None
            }
        },
        _ => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("expected string URL for '{attr_id}'"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn decode_json_map(
    val: serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<ContentValue> {
    let map = match val {
        serde_json::Value::Object(map) => map,
        _ => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("expected JSON object for field '{attr_id}'"),
                "invalid_field_value",
            ));
            return None;
        }
    };

    let mut json_fields = HashMap::with_capacity(map.len());
    let mut field_had_error = false;

    for (k, v) in map {
        match json_value_to_primitive(v, attr_id) {
            Ok(prim) => {
                json_fields.insert(k, prim);
            }
            Err(err) => {
                errors.push(err);
                field_had_error = true;
            }
        }
    }

    if field_had_error {
        None
    } else {
        Some(ContentValue::Scalar(DomainValue::Json(json_fields)))
    }
}

fn json_value_to_primitive(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, InvalidParam> {
    match val {
        serde_json::Value::Bool(b) => Ok(PrimitiveValue::Boolean(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PrimitiveValue::Integer(i))
            } else if let Ok(d) = Decimal::from_str_exact(&n.to_string()) {
                Ok(PrimitiveValue::Decimal(d))
            } else {
                Ok(PrimitiveValue::Text(n.to_string()))
            }
        }
        serde_json::Value::String(s) => Ok(PrimitiveValue::Text(s)),
        _ => Err(InvalidParam::new(
            attr_id.to_string(),
            format!("unsupported value type in JSON object for field '{attr_id}'"),
            "invalid_field_value",
        )),
    }
}

fn decode_primitive(
    val: serde_json::Value,
    pt: PrimitiveType,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    match pt {
        PrimitiveType::Uid => decode_uid(val, attr_id, errors),
        PrimitiveType::Text => decode_text(val, attr_id, errors),
        PrimitiveType::Uuid => decode_uuid(&val, attr_id, errors),
        PrimitiveType::Integer(size) => decode_integer(&val, size, attr_id, errors),
        PrimitiveType::Decimal { precision, scale } => {
            decode_decimal(&val, precision, scale, attr_id, errors)
        }
        PrimitiveType::Date => decode_date(&val, attr_id, errors),
        PrimitiveType::DateTime => decode_datetime(&val, attr_id, errors),
        PrimitiveType::Boolean => decode_boolean(&val, attr_id, errors),
    }
}

fn decode_uid(
    val: serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    match val {
        serde_json::Value::String(s) => Some(PrimitiveValue::Uid(s)),
        _ => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("expected string for UID '{attr_id}'"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn decode_text(
    val: serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    match val {
        serde_json::Value::String(s) => Some(PrimitiveValue::Text(s)),
        _ => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("expected string for '{attr_id}'"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn decode_uuid(
    val: &serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    let Some(s) = val.as_str() else {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!("expected string UUID for '{attr_id}'"),
            "invalid_field_value",
        ));
        return None;
    };

    match Uuid::parse_str(s) {
        Ok(u) => Some(PrimitiveValue::Uuid(u)),
        Err(e) => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("invalid UUID in '{attr_id}': {e}"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn decode_integer(
    val: &serde_json::Value,
    size: IntegerSize,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    let int_val = match val {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => s.parse::<i64>().ok(),
        _ => None,
    };

    let Some(i) = int_val else {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!("expected integer for '{attr_id}'"),
            "invalid_field_value",
        ));
        return None;
    };

    let in_bounds = match size {
        IntegerSize::I16 => (i64::from(i16::MIN)..=i64::from(i16::MAX)).contains(&i),
        IntegerSize::I32 => (i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&i),
        IntegerSize::I64 => true,
    };

    if in_bounds {
        Some(PrimitiveValue::Integer(i))
    } else {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!("integer {i} out of bounds for size {size:?} in '{attr_id}'"),
            "invalid_field_value",
        ));
        None
    }
}

fn decode_decimal(
    val: &serde_json::Value,
    precision: u8,
    scale: u8,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    let parsed = match val {
        serde_json::Value::String(s) => Decimal::from_str_exact(s).ok(),
        serde_json::Value::Number(n) => Decimal::from_str_exact(&n.to_string()).ok(),
        _ => None,
    };

    let Some(d) = parsed else {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!("expected decimal for '{attr_id}'"),
            "invalid_field_value",
        ));
        return None;
    };

    let prim = PrimitiveValue::Decimal(d);
    if prim.matches_primitive_type(&PrimitiveType::Decimal { precision, scale }) {
        Some(prim)
    } else {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!(
                "decimal '{d}' does not match precision {precision} and scale {scale} in '{attr_id}'"
            ),
            "invalid_field_value",
        ));
        None
    }
}

fn decode_date(
    val: &serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    let Some(s) = val.as_str() else {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!("expected string date 'YYYY-MM-DD' for '{attr_id}'"),
            "invalid_field_value",
        ));
        return None;
    };

    match NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        Ok(d) => Some(PrimitiveValue::Date(d)),
        Err(e) => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("invalid date format in '{attr_id}': {e}"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn decode_datetime(
    val: &serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    let Some(s) = val.as_str() else {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!("expected RFC 3339 datetime string for '{attr_id}'"),
            "invalid_field_value",
        ));
        return None;
    };

    match DateTime::parse_from_rfc3339(s) {
        Ok(dt) => Some(PrimitiveValue::DateTime(dt.with_timezone(&Utc))),
        Err(e) => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("invalid datetime format in '{attr_id}': {e}"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn decode_boolean(
    val: &serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<PrimitiveValue> {
    match val {
        serde_json::Value::Bool(b) => Some(PrimitiveValue::Boolean(*b)),
        _ => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("expected boolean for '{attr_id}'"),
                "invalid_field_value",
            ));
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Private Helpers - Relations
// ---------------------------------------------------------------------------

fn decode_single_relation(
    doc_type: &DocumentType,
    raw_key: String,
    val: serde_json::Value,
    relations: &mut HashMap<AttributeId, RelationAction>,
    errors: &mut Vec<InvalidParam>,
) {
    let attr_id = match AttributeId::try_new(raw_key.clone()) {
        Ok(id) => id,
        Err(_) => {
            errors.push(InvalidParam::new(
                raw_key,
                "invalid relation attribute name format",
                "invalid_attribute_name",
            ));
            return;
        }
    };

    if doc_type.relations.get(&attr_id).is_none() {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!("relation attribute '{attr_id}' is not declared in schema"),
            "unknown_attribute",
        ));
        return;
    }

    if let Some(action) = decode_relation_action(val, &attr_id, errors) {
        relations.insert(attr_id, action);
    }
}

fn decode_relation_action(
    val: serde_json::Value,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<RelationAction> {
    match val {
        serde_json::Value::Array(arr) => {
            parse_uuid_array(arr, attr_id, errors).map(RelationAction::Set)
        }
        serde_json::Value::Object(map) => decode_relation_object(map, attr_id, errors),
        serde_json::Value::Null => Some(RelationAction::Unset),
        _ => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("expected array or action object for relation '{attr_id}'"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn decode_relation_object(
    map: serde_json::Map<String, serde_json::Value>,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<RelationAction> {
    if map.len() != 1 {
        errors.push(InvalidParam::new(
            attr_id.to_string(),
            format!(
                "relation object for '{attr_id}' must specify exactly one action (set, connect, disconnect, or unset)"
            ),
            "invalid_field_value",
        ));
        return None;
    }

    let (action_key, action_val) = map.into_iter().next()?;
    match action_key.as_str() {
        "set" => match action_val {
            serde_json::Value::Array(arr) => {
                parse_uuid_array(arr, attr_id, errors).map(RelationAction::Set)
            }
            _ => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("expected array of UUIDs for 'set' in relation '{attr_id}'"),
                    "invalid_field_value",
                ));
                None
            }
        },
        "connect" => match action_val {
            serde_json::Value::Array(arr) => {
                parse_uuid_array(arr, attr_id, errors).map(RelationAction::Connect)
            }
            _ => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("expected array of UUIDs for 'connect' in relation '{attr_id}'"),
                    "invalid_field_value",
                ));
                None
            }
        },
        "disconnect" => match action_val {
            serde_json::Value::Array(arr) => {
                parse_uuid_array(arr, attr_id, errors).map(RelationAction::Disconnect)
            }
            _ => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("expected array of UUIDs for 'disconnect' in relation '{attr_id}'"),
                    "invalid_field_value",
                ));
                None
            }
        },
        "unset" => match action_val {
            serde_json::Value::Bool(true)
            | serde_json::Value::Array(_)
            | serde_json::Value::Null => Some(RelationAction::Unset),
            _ => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("expected true, null, or [] for 'unset' in relation '{attr_id}'"),
                    "invalid_field_value",
                ));
                None
            }
        },
        unknown => {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("unknown relation action '{unknown}' in relation '{attr_id}'"),
                "invalid_field_value",
            ));
            None
        }
    }
}

fn parse_uuid_array(
    arr: Vec<serde_json::Value>,
    attr_id: &AttributeId,
    errors: &mut Vec<InvalidParam>,
) -> Option<Vec<DocumentInstanceId>> {
    let mut uuids = Vec::with_capacity(arr.len());
    let mut had_error = false;

    for item in arr {
        let Some(s) = item.as_str() else {
            errors.push(InvalidParam::new(
                attr_id.to_string(),
                format!("relation target in '{attr_id}' must be a string UUID"),
                "invalid_field_value",
            ));
            had_error = true;
            continue;
        };

        match Uuid::parse_str(s) {
            Ok(u) => uuids.push(DocumentInstanceId::new(u)),
            Err(e) => {
                errors.push(InvalidParam::new(
                    attr_id.to_string(),
                    format!("invalid UUID in relation '{attr_id}': {e}"),
                    "invalid_field_value",
                ));
                had_error = true;
            }
        }
    }

    if had_error {
        None
    } else {
        Some(uuids)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::common::DisplayName;
    use domain::schema::attributes::{FieldDefinition, RelationDefinition, RelationType};
    use domain::schema::documents::{
        DocumentKind, DocumentTypeInfo, DocumentTypeOptions, DocumentTypeId,
    };
    use indexmap::IndexSet;

    fn build_test_doc_type() -> DocumentType {
        let mut fields = IndexSet::new();
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("title").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            unique: false,
            required: true,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("slug").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Uid),
            unique: true,
            required: true,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("views").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("price").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Decimal {
                precision: 8,
                scale: 2,
            }),
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("is-active").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Boolean),
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("published-date").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Date),
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("scheduled-at").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::DateTime),
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("contact-email").unwrap(),
            field_type: FieldType::Email,
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("website").unwrap(),
            field_type: FieldType::Url,
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("localized-body").unwrap(),
            field_type: FieldType::LocalizedText,
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });
        fields.insert(FieldDefinition {
            id: AttributeId::try_new("metadata").unwrap(),
            field_type: FieldType::Json,
            unique: false,
            required: false,
            constraints: std::collections::HashSet::new(),
        });

        let mut relations = IndexSet::new();
        relations.insert(RelationDefinition {
            id: AttributeId::try_new("tags").unwrap(),
            relation_type: RelationType::HasMany,
            target: DocumentTypeId::try_new("tag").unwrap(),
        });

        DocumentType {
            id: DocumentTypeId::try_new("article").unwrap(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Article").unwrap(),
                singular_name: DocumentTypeId::try_new("article").unwrap(),
                plural_name: DocumentTypeId::try_new("articles").unwrap(),
                description: None,
                display_fields: vec![AttributeId::try_new("title").unwrap()],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
            relations,
        }
    }

    #[test]
    fn test_decode_all_primitives_success() {
        let doc_type = build_test_doc_type();
        let mut raw = serde_json::Map::new();
        raw.insert(
            "title".into(),
            serde_json::Value::String("Rust 2026".into()),
        );
        raw.insert("slug".into(), serde_json::Value::String("rust-2026".into()));
        raw.insert("views".into(), serde_json::json!(100));
        raw.insert("price".into(), serde_json::json!("99.50"));
        raw.insert("is-active".into(), serde_json::Value::Bool(true));
        raw.insert(
            "published-date".into(),
            serde_json::Value::String("2026-10-27".into()),
        );
        raw.insert(
            "scheduled-at".into(),
            serde_json::Value::String("2026-10-27T12:00:00Z".into()),
        );
        raw.insert(
            "contact-email".into(),
            serde_json::Value::String("test@luminair.io".into()),
        );
        raw.insert(
            "website".into(),
            serde_json::Value::String("https://luminair.io".into()),
        );

        let decoded = decode_fields_map(&doc_type, raw).expect("decode fields");
        assert_eq!(
            decoded.get(&AttributeId::try_new("title").unwrap()),
            Some(&ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Text("Rust 2026".into())
            )))
        );
        assert_eq!(
            decoded.get(&AttributeId::try_new("views").unwrap()),
            Some(&ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Integer(100)
            )))
        );
        assert_eq!(
            decoded.get(&AttributeId::try_new("price").unwrap()),
            Some(&ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Decimal(Decimal::new(9950, 2))
            )))
        );
        assert_eq!(
            decoded.get(&AttributeId::try_new("is-active").unwrap()),
            Some(&ContentValue::Scalar(DomainValue::Primitive(
                PrimitiveValue::Boolean(true)
            )))
        );
    }

    #[test]
    fn test_decode_localized_text_and_json() {
        let doc_type = build_test_doc_type();
        let mut raw = serde_json::Map::new();
        raw.insert(
            "localized-body".into(),
            serde_json::json!({
                "en": "Hello world",
                "fr": "Bonjour le monde"
            }),
        );
        raw.insert(
            "metadata".into(),
            serde_json::json!({
                "readingTime": 5,
                "featured": true,
                "author": "Alice"
            }),
        );

        let decoded = decode_fields_map(&doc_type, raw).expect("decode fields");

        let loc_val = decoded
            .get(&AttributeId::try_new("localized-body").unwrap())
            .expect("localized-body");
        match loc_val {
            ContentValue::LocalizedText(map) => {
                assert_eq!(
                    map.get(&LocaleId::try_new("en").unwrap()),
                    Some(&"Hello world".to_string())
                );
                assert_eq!(
                    map.get(&LocaleId::try_new("fr").unwrap()),
                    Some(&"Bonjour le monde".to_string())
                );
            }
            _ => panic!("expected LocalizedText"),
        }

        let json_val = decoded
            .get(&AttributeId::try_new("metadata").unwrap())
            .expect("metadata");
        match json_val {
            ContentValue::Scalar(DomainValue::Json(map)) => {
                assert_eq!(map.get("readingTime"), Some(&PrimitiveValue::Integer(5)));
                assert_eq!(map.get("featured"), Some(&PrimitiveValue::Boolean(true)));
                assert_eq!(
                    map.get("author"),
                    Some(&PrimitiveValue::Text("Alice".into()))
                );
            }
            _ => panic!("expected Json"),
        }
    }

    #[test]
    fn test_decode_relations_actions() {
        let doc_type = build_test_doc_type();
        let u1 = Uuid::now_v7();
        let u2 = Uuid::now_v7();

        // Shorthand array -> Set
        let mut raw_shorthand = serde_json::Map::new();
        raw_shorthand.insert(
            "tags".into(),
            serde_json::json!([u1.to_string(), u2.to_string()]),
        );
        let rels_set = decode_relations_map(&doc_type, raw_shorthand).expect("decode shorthand");
        assert_eq!(
            rels_set.get(&AttributeId::try_new("tags").unwrap()),
            Some(&RelationAction::Set(vec![
                DocumentInstanceId::new(u1),
                DocumentInstanceId::new(u2)
            ]))
        );

        // Explicit connect
        let mut raw_connect = serde_json::Map::new();
        raw_connect.insert(
            "tags".into(),
            serde_json::json!({ "connect": [u1.to_string()] }),
        );
        let rels_connect =
            decode_relations_map(&doc_type, raw_connect).expect("decode explicit connect");
        assert_eq!(
            rels_connect.get(&AttributeId::try_new("tags").unwrap()),
            Some(&RelationAction::Connect(vec![DocumentInstanceId::new(
                u1
            )]))
        );

        // Explicit disconnect
        let mut raw_disconnect = serde_json::Map::new();
        raw_disconnect.insert(
            "tags".into(),
            serde_json::json!({ "disconnect": [u2.to_string()] }),
        );
        let rels_disconnect =
            decode_relations_map(&doc_type, raw_disconnect).expect("decode explicit disconnect");
        assert_eq!(
            rels_disconnect.get(&AttributeId::try_new("tags").unwrap()),
            Some(&RelationAction::Disconnect(vec![DocumentInstanceId::new(
                u2
            )]))
        );

        // Explicit unset
        let mut raw_unset = serde_json::Map::new();
        raw_unset.insert("tags".into(), serde_json::json!({ "unset": true }));
        let rels_unset = decode_relations_map(&doc_type, raw_unset).expect("decode unset");
        assert_eq!(
            rels_unset.get(&AttributeId::try_new("tags").unwrap()),
            Some(&RelationAction::Unset)
        );
    }

    #[test]
    fn test_multi_field_error_accumulation() {
        let doc_type = build_test_doc_type();
        let mut fields_raw = serde_json::Map::new();
        // 1. Unknown attribute
        fields_raw.insert(
            "unknown-field".into(),
            serde_json::Value::String("val".into()),
        );
        // 2. Type mismatch: integer expected, got string non-number
        fields_raw.insert("views".into(), serde_json::Value::String("abc".into()));
        // 3. Invalid email
        fields_raw.insert("contact-email".into(), serde_json::json!("not-an-email"));

        let mut relations_raw = serde_json::Map::new();
        // 4. Unknown relation
        relations_raw.insert("unknown-rel".into(), serde_json::json!([]));
        // 5. Invalid UUID in relation
        relations_raw.insert("tags".into(), serde_json::json!(["not-a-uuid"]));

        let result = decode_document_payload(&doc_type, fields_raw, Some(relations_raw));
        let err = result.expect_err("should fail with accumulated validation errors");

        match err {
            ApplicationError::Validation(params) => {
                assert_eq!(params.len(), 5);
                let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                assert!(names.contains(&"unknown-field"));
                assert!(names.contains(&"views"));
                assert!(names.contains(&"contact-email"));
                assert!(names.contains(&"unknown-rel"));
                assert!(names.contains(&"tags"));
            }
            other => panic!("expected ApplicationError::Validation, got {other:?}"),
        }
    }
}
