//! Schema-driven JSONB, ActorId URN, status, reason, and permission codecs.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use indexmap::IndexMap;
use rust_decimal::Decimal;
use uuid::Uuid;

use domain::auth::access_request::AccessRequestStatus;
use domain::auth::role::{Permission, RoleId};
use domain::auth::user::ActorId;
use domain::common::{Email, Url};
use domain::content::header::{DocumentStatus, SnapshotReason};
use domain::content::instance::{DocumentInstanceId, PublicationState, ResolvedRelation};
use domain::content::values::{ContentValue, DomainValue, PrimitiveValue};
use domain::schema::{AttributeId, DocumentTypeId, FieldType, PrimitiveType};
use domain::system::LocaleId;

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::DocumentTypeNaming;

// ---------------------------------------------------------------------------
// Public Functions - Actor URN
// ---------------------------------------------------------------------------

/// Parses an [`ActorId`] from its URN string representation.
pub fn parse_actor_urn(urn: &str) -> Result<ActorId, PersistenceError> {
    ActorId::from_urn(urn).map_err(PersistenceError::Domain)
}

/// Parses an optional URN string into an optional [`ActorId`].
pub fn parse_optional_actor_urn(urn: Option<String>) -> Result<Option<ActorId>, PersistenceError> {
    urn.as_deref().map(parse_actor_urn).transpose()
}

// ---------------------------------------------------------------------------
// Public Functions - DocumentStatus & PublicationState Wire
// ---------------------------------------------------------------------------

/// Converts a [`DocumentStatus`] into its database string representation.
#[must_use]
pub fn status_to_db_str(status: DocumentStatus) -> &'static str {
    match status {
        DocumentStatus::Draft => "draft",
        DocumentStatus::Published => "published",
        DocumentStatus::Modified => "modified",
        DocumentStatus::Unpublished => "unpublished",
    }
}

/// Parses a [`DocumentStatus`] from its database string representation.
pub fn status_from_db_str(s: &str) -> Result<DocumentStatus, PersistenceError> {
    match s {
        "draft" => Ok(DocumentStatus::Draft),
        "published" => Ok(DocumentStatus::Published),
        "modified" => Ok(DocumentStatus::Modified),
        "unpublished" => Ok(DocumentStatus::Unpublished),
        unknown => Err(PersistenceError::Validation(format!(
            "unknown document status: '{unknown}'"
        ))),
    }
}

/// Maps a working [`PublicationState`] to its `document_drafts` status string and live revision.
pub fn publication_state_to_draft_status_str(
    state: &PublicationState,
) -> Result<(&'static str, Option<i32>), PersistenceError> {
    match state {
        PublicationState::Draft => Ok(("draft", None)),
        PublicationState::Modified { live_revision, .. } => {
            let rev = i32::try_from(*live_revision).unwrap_or(i32::MAX);
            Ok(("modified", Some(rev)))
        }
        PublicationState::Unpublished {
            last_published_revision,
            ..
        } => {
            let rev = i32::try_from(*last_published_revision).unwrap_or(i32::MAX);
            Ok(("unpublished", Some(rev)))
        }
        PublicationState::Published { .. } => Err(PersistenceError::Validation(
            "cannot store clean Published state in document_drafts".into(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Public Functions - SnapshotReason Wire
// ---------------------------------------------------------------------------

/// Converts a [`SnapshotReason`] into its database string representation.
#[must_use]
pub fn snapshot_reason_to_db_str(reason: SnapshotReason) -> &'static str {
    match reason {
        SnapshotReason::Publish => "publish",
        SnapshotReason::Unpublish => "unpublish",
    }
}

/// Parses a [`SnapshotReason`] from its database string representation.
pub fn snapshot_reason_from_db_str(s: &str) -> Result<SnapshotReason, PersistenceError> {
    match s {
        "publish" => Ok(SnapshotReason::Publish),
        "unpublish" => Ok(SnapshotReason::Unpublish),
        unknown => Err(PersistenceError::Validation(format!(
            "unknown snapshot reason: '{unknown}'"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Public Functions - RBAC & Permissions
// ---------------------------------------------------------------------------

/// Decomposes a [`Permission`] into an action string and an optional target document type identifier.
#[must_use]
pub fn permission_to_db_tuple(perm: &Permission) -> (&'static str, Option<String>) {
    match perm {
        Permission::ManageSchema => ("ManageSchema", None),
        Permission::CreateDocument(opt) => (
            "CreateDocument",
            opt.as_ref().map(|t| t.as_ref().to_string()),
        ),
        Permission::ReadDocument(opt) => {
            ("ReadDocument", opt.as_ref().map(|t| t.as_ref().to_string()))
        }
        Permission::UpdateDocument(opt) => (
            "UpdateDocument",
            opt.as_ref().map(|t| t.as_ref().to_string()),
        ),
        Permission::DeleteDocument(opt) => (
            "DeleteDocument",
            opt.as_ref().map(|t| t.as_ref().to_string()),
        ),
        Permission::PublishDocument(opt) => (
            "PublishDocument",
            opt.as_ref().map(|t| t.as_ref().to_string()),
        ),
        Permission::ManageRoles => ("ManageRoles", None),
        Permission::ManageUsers => ("ManageUsers", None),
    }
}

/// Reconstitutes a [`Permission`] from database action and document type strings.
pub fn permission_from_db_row(
    action: &str,
    doc_type_id: Option<String>,
) -> Result<Permission, PersistenceError> {
    let parse_type_id = |s: Option<String>| -> Result<Option<DocumentTypeId>, PersistenceError> {
        s.map(|v| DocumentTypeId::try_new(v).map_err(|e| PersistenceError::Validation(e.to_string())))
            .transpose()
    };

    match action {
        "ManageSchema" => Ok(Permission::ManageSchema),
        "CreateDocument" => Ok(Permission::CreateDocument(parse_type_id(doc_type_id)?)),
        "ReadDocument" => Ok(Permission::ReadDocument(parse_type_id(doc_type_id)?)),
        "UpdateDocument" => Ok(Permission::UpdateDocument(parse_type_id(doc_type_id)?)),
        "DeleteDocument" => Ok(Permission::DeleteDocument(parse_type_id(doc_type_id)?)),
        "PublishDocument" => Ok(Permission::PublishDocument(parse_type_id(doc_type_id)?)),
        "ManageRoles" => Ok(Permission::ManageRoles),
        "ManageUsers" => Ok(Permission::ManageUsers),
        unknown => Err(PersistenceError::Validation(format!(
            "unknown permission action: '{unknown}'"
        ))),
    }
}

/// Maps an [`AccessRequestStatus`] to its database wire representation.
#[must_use]
pub fn access_request_status_to_db(status: &AccessRequestStatus) -> (&'static str, Option<&str>) {
    match status {
        AccessRequestStatus::Pending => ("pending", None),
        AccessRequestStatus::Approved => ("approved", None),
        AccessRequestStatus::Rejected { reason } => ("rejected", reason.as_deref()),
    }
}

/// Parses an [`AccessRequestStatus`] from database status and rejection reason.
pub fn access_request_status_from_db(
    status: &str,
    rejection_reason: Option<String>,
) -> Result<AccessRequestStatus, PersistenceError> {
    match status {
        "pending" => Ok(AccessRequestStatus::Pending),
        "approved" => Ok(AccessRequestStatus::Approved),
        "rejected" => Ok(AccessRequestStatus::Rejected {
            reason: rejection_reason,
        }),
        unknown => Err(PersistenceError::Validation(format!(
            "unknown access request status: '{unknown}'"
        ))),
    }
}

/// Serializes assigned role IDs to a JSONB array.
pub fn serialize_assigned_roles_json(
    roles: &[RoleId],
) -> Result<serde_json::Value, PersistenceError> {
    serde_json::to_value(roles).map_err(PersistenceError::Json)
}

/// Deserializes assigned role IDs from a JSONB array.
pub fn deserialize_assigned_roles_json(
    val: serde_json::Value,
) -> Result<Vec<RoleId>, PersistenceError> {
    if val.is_null() {
        return Ok(Vec::new());
    }
    serde_json::from_value(val).map_err(PersistenceError::Json)
}

// ---------------------------------------------------------------------------
// Public Functions - Display Values & Relations JSONB
// ---------------------------------------------------------------------------

/// Serializes display values to JSONB.
pub fn serialize_display_values_json(
    display_values: &IndexMap<AttributeId, String>,
) -> Result<serde_json::Value, PersistenceError> {
    serde_json::to_value(display_values).map_err(PersistenceError::Json)
}

/// Deserializes display values from JSONB.
pub fn deserialize_display_values_json(
    val: serde_json::Value,
) -> Result<IndexMap<AttributeId, String>, PersistenceError> {
    if val.is_null() {
        return Ok(IndexMap::new());
    }
    serde_json::from_value(val).map_err(PersistenceError::Json)
}

/// Serializes document relations to a JSON object: `{ "<attr_id>": ["<uuid1>", "<uuid2>"] }`.
pub fn serialize_relations_json(
    relations: &HashMap<AttributeId, Vec<ResolvedRelation>>,
) -> Result<serde_json::Value, PersistenceError> {
    let mut map = serde_json::Map::with_capacity(relations.len());
    for (attr_id, list) in relations {
        let targets: Vec<serde_json::Value> = list
            .iter()
            .map(|r| serde_json::Value::String(r.target_instance_id.to_string()))
            .collect();
        map.insert(
            attr_id.as_ref().to_string(),
            serde_json::Value::Array(targets),
        );
    }
    Ok(serde_json::Value::Object(map))
}

/// Deserializes document relations from a JSON object: `{ "<attr_id>": ["<uuid1>", "<uuid2>"] }`.
pub fn deserialize_relations_json(
    val: serde_json::Value,
) -> Result<HashMap<AttributeId, Vec<ResolvedRelation>>, PersistenceError> {
    if val.is_null() {
        return Ok(HashMap::new());
    }
    let obj = match val {
        serde_json::Value::Object(map) => map,
        _ => {
            return Err(PersistenceError::Validation(
                "relations JSON must be an object".into(),
            ));
        }
    };

    let mut relations = HashMap::with_capacity(obj.len());
    for (attr_key, targets_val) in obj {
        let attr_id = AttributeId::try_new(attr_key)
            .map_err(|e| PersistenceError::Validation(e.to_string()))?;
        let resolved_list = parse_relation_targets_array(&attr_id, targets_val)?;
        relations.insert(attr_id, resolved_list);
    }
    Ok(relations)
}

// ---------------------------------------------------------------------------
// Public Functions - Schema-Driven Fields JSONB
// ---------------------------------------------------------------------------

/// Serializes a fields map into a JSON object.
pub fn serialize_fields_json(
    fields: &HashMap<AttributeId, ContentValue>,
) -> Result<serde_json::Value, PersistenceError> {
    let mut map = serde_json::Map::with_capacity(fields.len());
    for (attr_id, content_val) in fields {
        map.insert(
            attr_id.as_ref().to_string(),
            content_value_to_json(content_val),
        );
    }
    Ok(serde_json::Value::Object(map))
}

/// Deserializes a content fields JSON object into domain [`ContentValue`]s driven by schema naming.
pub fn deserialize_fields_json(
    val: serde_json::Value,
    type_naming: &DocumentTypeNaming,
) -> Result<HashMap<AttributeId, ContentValue>, PersistenceError> {
    if val.is_null() {
        return Ok(HashMap::new());
    }
    let obj = match val {
        serde_json::Value::Object(map) => map,
        _ => {
            return Err(PersistenceError::Validation(
                "content fields JSON must be an object".into(),
            ));
        }
    };

    let mut fields = HashMap::with_capacity(obj.len());
    for (attr_key, field_val) in obj {
        let attr_id = AttributeId::try_new(attr_key)
            .map_err(|e| PersistenceError::Validation(e.to_string()))?;
        let col = type_naming.field_column(&attr_id).ok_or_else(|| {
            PersistenceError::Validation(format!(
                "unknown attribute '{attr_id}' in content JSON for document type '{}'",
                type_naming.document_type_id
            ))
        })?;

        let content_val = deserialize_field_value(field_val, col.field_type, &attr_id)?;
        fields.insert(attr_id, content_val);
    }
    Ok(fields)
}

// ---------------------------------------------------------------------------
// Private Helpers - Relations
// ---------------------------------------------------------------------------

fn parse_relation_targets_array(
    attr_id: &AttributeId,
    val: serde_json::Value,
) -> Result<Vec<ResolvedRelation>, PersistenceError> {
    let arr = match val {
        serde_json::Value::Array(arr) => arr,
        _ => {
            return Err(PersistenceError::Validation(format!(
                "relation targets for '{attr_id}' must be an array"
            )));
        }
    };

    let mut list = Vec::with_capacity(arr.len());
    for item in arr {
        let s = item.as_str().ok_or_else(|| {
            PersistenceError::Validation(format!(
                "relation target in '{attr_id}' must be a string UUID"
            ))
        })?;
        let target_uuid = Uuid::parse_str(s).map_err(|e| {
            PersistenceError::Validation(format!(
                "invalid UUID in relation target for '{attr_id}': {e}"
            ))
        })?;
        list.push(ResolvedRelation {
            attribute_id: attr_id.clone(),
            target_instance_id: DocumentInstanceId::new(target_uuid),
        });
    }
    Ok(list)
}

// ---------------------------------------------------------------------------
// Private Helpers - Fields Serialization
// ---------------------------------------------------------------------------

fn content_value_to_json(cv: &ContentValue) -> serde_json::Value {
    match cv {
        ContentValue::Null => serde_json::Value::Null,
        ContentValue::LocalizedText(map) => {
            let mut obj = serde_json::Map::with_capacity(map.len());
            for (loc, text) in map {
                obj.insert(
                    loc.as_ref().to_string(),
                    serde_json::Value::String(text.clone()),
                );
            }
            serde_json::Value::Object(obj)
        }
        ContentValue::Scalar(dv) => domain_value_to_json(dv),
    }
}

fn domain_value_to_json(dv: &DomainValue) -> serde_json::Value {
    match dv {
        DomainValue::Primitive(p) => primitive_value_to_json(p),
        DomainValue::Email(e) => serde_json::Value::String(e.as_ref().to_string()),
        DomainValue::Url(u) => serde_json::Value::String(u.as_ref().to_string()),
        DomainValue::Json(map) => {
            let mut obj = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                obj.insert(k.clone(), primitive_value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
    }
}

fn primitive_value_to_json(prim: &PrimitiveValue) -> serde_json::Value {
    match prim {
        PrimitiveValue::Text(s) | PrimitiveValue::Uid(s) => serde_json::Value::String(s.clone()),
        PrimitiveValue::Uuid(u) => serde_json::Value::String(u.to_string()),
        PrimitiveValue::Integer(i) => serde_json::Value::Number((*i).into()),
        PrimitiveValue::Decimal(d) => serde_json::Value::String(d.to_string()),
        PrimitiveValue::Date(d) => serde_json::Value::String(d.format("%Y-%m-%d").to_string()),
        PrimitiveValue::DateTime(dt) => serde_json::Value::String(dt.to_rfc3339()),
        PrimitiveValue::Boolean(b) => serde_json::Value::Bool(*b),
    }
}

// ---------------------------------------------------------------------------
// Private Helpers - Fields Deserialization
// ---------------------------------------------------------------------------

fn deserialize_field_value(
    val: serde_json::Value,
    field_type: FieldType,
    attr_id: &AttributeId,
) -> Result<ContentValue, PersistenceError> {
    if val.is_null() {
        return Ok(ContentValue::Null);
    }

    match field_type {
        FieldType::LocalizedText => deserialize_localized_text(val, attr_id),
        FieldType::Email => deserialize_email(val, attr_id),
        FieldType::Url => deserialize_url(val, attr_id),
        FieldType::Json => deserialize_json_map(val, attr_id),
        FieldType::Primitive(pt) => {
            let prim = deserialize_primitive(val, pt, attr_id)?;
            Ok(ContentValue::Scalar(DomainValue::Primitive(prim)))
        }
    }
}

pub(crate) fn deserialize_localized_text(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<ContentValue, PersistenceError> {
    let obj = val.as_object().ok_or_else(|| {
        PersistenceError::Validation(format!(
            "expected JSON object for localized text field '{attr_id}'"
        ))
    })?;

    let mut map = HashMap::with_capacity(obj.len());
    for (loc_str, text_val) in obj {
        let loc = LocaleId::try_new(loc_str)
            .map_err(|e| PersistenceError::Validation(e.to_string()))?;
        let text = text_val.as_str().ok_or_else(|| {
            PersistenceError::Validation(format!(
                "expected string value for locale '{loc}' in field '{attr_id}'"
            ))
        })?;
        map.insert(loc, text.to_string());
    }
    Ok(ContentValue::LocalizedText(map))
}

fn deserialize_email(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<ContentValue, PersistenceError> {
    let s = val.as_str().ok_or_else(|| {
        PersistenceError::Validation(format!("expected string for email field '{attr_id}'"))
    })?;
    let email = Email::try_new(s)
        .map_err(|e| PersistenceError::Validation(e.to_string()))?;
    Ok(ContentValue::Scalar(DomainValue::Email(email)))
}

fn deserialize_url(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<ContentValue, PersistenceError> {
    let s = val.as_str().ok_or_else(|| {
        PersistenceError::Validation(format!("expected string for url field '{attr_id}'"))
    })?;
    let url = Url::try_new(s)
        .map_err(|e| PersistenceError::Validation(e.to_string()))?;
    Ok(ContentValue::Scalar(DomainValue::Url(url)))
}

pub(crate) fn deserialize_json_map(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<ContentValue, PersistenceError> {
    let obj = val.as_object().ok_or_else(|| {
        PersistenceError::Validation(format!("expected JSON object for json field '{attr_id}'"))
    })?;

    let mut map = HashMap::with_capacity(obj.len());
    for (key, item) in obj {
        let prim = json_value_to_primitive(item, attr_id)?;
        map.insert(key.clone(), prim);
    }
    Ok(ContentValue::Scalar(DomainValue::Json(map)))
}

fn json_value_to_primitive(
    val: &serde_json::Value,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, PersistenceError> {
    match val {
        serde_json::Value::Bool(b) => Ok(PrimitiveValue::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PrimitiveValue::Integer(i))
            } else if let Ok(d) = Decimal::from_str_exact(&n.to_string()) {
                Ok(PrimitiveValue::Decimal(d))
            } else {
                Ok(PrimitiveValue::Text(n.to_string()))
            }
        }
        serde_json::Value::String(s) => Ok(PrimitiveValue::Text(s.clone())),
        _ => Err(PersistenceError::Validation(format!(
            "unsupported value type in JSON map for field '{attr_id}'"
        ))),
    }
}

fn deserialize_primitive(
    val: serde_json::Value,
    pt: PrimitiveType,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, PersistenceError> {
    match pt {
        PrimitiveType::Uid => parse_string_value(val, attr_id).map(PrimitiveValue::Uid),
        PrimitiveType::Text => parse_string_value(val, attr_id).map(PrimitiveValue::Text),
        PrimitiveType::Uuid => parse_uuid_value(val, attr_id),
        PrimitiveType::Integer(_) => parse_integer_value(val, attr_id),
        PrimitiveType::Decimal { .. } => parse_decimal_value(val, attr_id),
        PrimitiveType::Date => parse_date_value(val, attr_id),
        PrimitiveType::DateTime => parse_datetime_value(val, attr_id),
        PrimitiveType::Boolean => parse_boolean_value(val, attr_id),
    }
}

fn parse_string_value(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<String, PersistenceError> {
    val.as_str()
        .map(ToString::to_string)
        .ok_or_else(|| PersistenceError::Validation(format!("expected string for '{attr_id}'")))
}

fn parse_uuid_value(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, PersistenceError> {
    let s = parse_string_value(val, attr_id)?;
    Uuid::parse_str(&s)
        .map(PrimitiveValue::Uuid)
        .map_err(|e| PersistenceError::Validation(format!("invalid UUID in '{attr_id}': {e}")))
}

fn parse_integer_value(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, PersistenceError> {
    if let Some(i) = val.as_i64() {
        return Ok(PrimitiveValue::Integer(i));
    }
    if let Some(s) = val.as_str() {
        return s
            .parse::<i64>()
            .map(PrimitiveValue::Integer)
            .map_err(|e| PersistenceError::Validation(format!("invalid int in '{attr_id}': {e}")));
    }
    Err(PersistenceError::Validation(format!(
        "expected integer for '{attr_id}'"
    )))
}

fn parse_decimal_value(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, PersistenceError> {
    if let Some(s) = val.as_str() {
        return Decimal::from_str_exact(s).map(PrimitiveValue::Decimal).map_err(|e| {
            PersistenceError::Validation(format!("invalid decimal in '{attr_id}': {e}"))
        });
    }
    if let Some(n) = val.as_number() {
        return Decimal::from_str_exact(&n.to_string())
            .map(PrimitiveValue::Decimal)
            .map_err(|e| {
                PersistenceError::Validation(format!("invalid decimal in '{attr_id}': {e}"))
            });
    }
    Err(PersistenceError::Validation(format!(
        "expected decimal string or number for '{attr_id}'"
    )))
}

fn parse_date_value(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, PersistenceError> {
    let s = parse_string_value(val, attr_id)?;
    NaiveDate::parse_from_str(&s, "%Y-%m-%d")
        .map(PrimitiveValue::Date)
        .map_err(|e| PersistenceError::Validation(format!("invalid date in '{attr_id}': {e}")))
}

fn parse_datetime_value(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, PersistenceError> {
    let s = parse_string_value(val, attr_id)?;
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| PrimitiveValue::DateTime(dt.with_timezone(&Utc)))
        .map_err(|e| PersistenceError::Validation(format!("invalid datetime in '{attr_id}': {e}")))
}

fn parse_boolean_value(
    val: serde_json::Value,
    attr_id: &AttributeId,
) -> Result<PrimitiveValue, PersistenceError> {
    val.as_bool()
        .map(PrimitiveValue::Boolean)
        .ok_or_else(|| PersistenceError::Validation(format!("expected boolean for '{attr_id}'")))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::common::DisplayName;
    use domain::schema::{DocumentKind, DocumentType, DocumentTypeInfo, DocumentTypeOptions, IntegerSize};
    use indexmap::IndexSet;

    fn make_test_type_naming() -> DocumentTypeNaming {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let _doc_type = DocumentType {
            id: type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Article").unwrap(),
                singular_name: type_id.clone(),
                plural_name: DocumentTypeId::try_new("articles").unwrap(),
                description: None,
                display_fields: vec![],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: IndexSet::new(),
            relations: IndexSet::new(),
        };
        let mut naming = crate::persistence::naming::DocumentTypeNaming {
            document_type_id: type_id,
            table_name: "articles",
            table_iden: crate::persistence::naming::StaticIden("articles"),
            field_columns: IndexMap::new(),
            wide_upsert_columns: &[],
            wide_update_columns: &[],
            relations: IndexMap::new(),
        };
        naming.field_columns.insert(
            AttributeId::try_new("title").unwrap(),
            crate::persistence::naming::FieldColumnNaming {
                column_name: "title",
                column_iden: crate::persistence::naming::StaticIden("title"),
                field_type: FieldType::Primitive(PrimitiveType::Text),
            },
        );
        naming.field_columns.insert(
            AttributeId::try_new("view-count").unwrap(),
            crate::persistence::naming::FieldColumnNaming {
                column_name: "view_count",
                column_iden: crate::persistence::naming::StaticIden("view_count"),
                field_type: FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)),
            },
        );
        naming.field_columns.insert(
            AttributeId::try_new("published-date").unwrap(),
            crate::persistence::naming::FieldColumnNaming {
                column_name: "published_date",
                column_iden: crate::persistence::naming::StaticIden("published_date"),
                field_type: FieldType::Primitive(PrimitiveType::Date),
            },
        );
        naming.field_columns.insert(
            AttributeId::try_new("extra-meta").unwrap(),
            crate::persistence::naming::FieldColumnNaming {
                column_name: "extra_meta",
                column_iden: crate::persistence::naming::StaticIden("extra_meta"),
                field_type: FieldType::Json,
            },
        );
        naming
    }

    #[test]
    fn test_actor_urn_roundtrip() {
        let urn = "user:usr_12345";
        let actor = parse_actor_urn(urn).unwrap();
        assert_eq!(actor.to_urn(), urn);

        let opt = parse_optional_actor_urn(Some(urn.into())).unwrap();
        assert_eq!(opt, Some(actor));

        let none = parse_optional_actor_urn(None).unwrap();
        assert_eq!(none, None);
    }

    #[test]
    fn test_status_conversions() {
        assert_eq!(status_to_db_str(DocumentStatus::Draft), "draft");
        assert_eq!(status_to_db_str(DocumentStatus::Published), "published");
        assert_eq!(status_to_db_str(DocumentStatus::Modified), "modified");
        assert_eq!(status_to_db_str(DocumentStatus::Unpublished), "unpublished");

        assert_eq!(status_from_db_str("draft").unwrap(), DocumentStatus::Draft);
        assert_eq!(status_from_db_str("modified").unwrap(), DocumentStatus::Modified);
        assert!(status_from_db_str("invalid").is_err());
    }

    #[test]
    fn test_publication_state_to_draft_status() {
        let draft = PublicationState::Draft;
        assert_eq!(publication_state_to_draft_status_str(&draft).unwrap(), ("draft", None));

        let mod_state = PublicationState::Modified {
            live_revision: 5,
            published_at: Utc::now(),
            published_by: ActorId::System,
            draft_version: 6,
        };
        assert_eq!(
            publication_state_to_draft_status_str(&mod_state).unwrap(),
            ("modified", Some(5))
        );

        let pub_state = PublicationState::Published {
            revision: 1,
            published_at: Utc::now(),
            published_by: ActorId::System,
        };
        assert!(publication_state_to_draft_status_str(&pub_state).is_err());
    }

    #[test]
    fn test_snapshot_reason_roundtrip() {
        assert_eq!(snapshot_reason_to_db_str(SnapshotReason::Publish), "publish");
        assert_eq!(snapshot_reason_to_db_str(SnapshotReason::Unpublish), "unpublish");
        assert_eq!(snapshot_reason_from_db_str("publish").unwrap(), SnapshotReason::Publish);
        assert_eq!(snapshot_reason_from_db_str("unpublish").unwrap(), SnapshotReason::Unpublish);
        assert!(snapshot_reason_from_db_str("bogus").is_err());
    }

    #[test]
    fn test_permission_wire_roundtrip() {
        let p1 = Permission::ManageSchema;
        let (action, doc) = permission_to_db_tuple(&p1);
        assert_eq!(action, "ManageSchema");
        assert_eq!(doc, None);
        assert_eq!(permission_from_db_row(action, doc).unwrap(), p1);

        let p2 = Permission::CreateDocument(Some(DocumentTypeId::try_new("post").unwrap()));
        let (action2, doc2) = permission_to_db_tuple(&p2);
        assert_eq!(action2, "CreateDocument");
        assert_eq!(doc2, Some("post".into()));
        assert_eq!(permission_from_db_row(action2, doc2).unwrap(), p2);
    }

    #[test]
    fn test_access_request_status_roundtrip() {
        let (s, r) = access_request_status_to_db(&AccessRequestStatus::Pending);
        assert_eq!(s, "pending");
        assert_eq!(r, None);
        assert_eq!(
            access_request_status_from_db(s, r.map(ToString::to_string)).unwrap(),
            AccessRequestStatus::Pending
        );

        let rejected = AccessRequestStatus::Rejected {
            reason: Some("No access".into()),
        };
        let (s2, r2) = access_request_status_to_db(&rejected);
        assert_eq!(s2, "rejected");
        assert_eq!(r2, Some("No access"));
        assert_eq!(
            access_request_status_from_db(s2, r2.map(ToString::to_string)).unwrap(),
            AccessRequestStatus::Rejected {
                reason: Some("No access".into())
            }
        );
    }

    #[test]
    fn test_assigned_roles_json_roundtrip() {
        let roles = vec![RoleId::new(Uuid::now_v7()), RoleId::new(Uuid::now_v7())];
        let val = serialize_assigned_roles_json(&roles).unwrap();
        let decoded = deserialize_assigned_roles_json(val).unwrap();
        assert_eq!(roles, decoded);
    }

    #[test]
    fn test_display_values_json_roundtrip() {
        let mut map = IndexMap::new();
        map.insert(AttributeId::try_new("title").unwrap(), "My Title".into());
        let val = serialize_display_values_json(&map).unwrap();
        let decoded = deserialize_display_values_json(val).unwrap();
        assert_eq!(map, decoded);
    }

    #[test]
    fn test_relations_json_roundtrip() {
        let attr = AttributeId::try_new("tags").unwrap();
        let target_id = DocumentInstanceId::new(Uuid::now_v7());
        let mut relations = HashMap::new();
        relations.insert(
            attr.clone(),
            vec![ResolvedRelation {
                attribute_id: attr.clone(),
                target_instance_id: target_id,
            }],
        );

        let val = serialize_relations_json(&relations).unwrap();
        let decoded = deserialize_relations_json(val).unwrap();
        assert_eq!(relations, decoded);
    }

    #[test]
    fn test_fields_json_roundtrip() {
        let type_naming = make_test_type_naming();
        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Hello".into()))),
        );
        fields.insert(
            AttributeId::try_new("view-count").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(100))),
        );
        fields.insert(
            AttributeId::try_new("published-date").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Date(
                NaiveDate::from_ymd_opt(2026, 9, 24).unwrap(),
            ))),
        );
        let mut meta = HashMap::new();
        meta.insert("views".into(), PrimitiveValue::Integer(42));
        meta.insert("featured".into(), PrimitiveValue::Boolean(true));
        fields.insert(
            AttributeId::try_new("extra-meta").unwrap(),
            ContentValue::Scalar(DomainValue::Json(meta)),
        );

        let val = serialize_fields_json(&fields).unwrap();
        let decoded = deserialize_fields_json(val, &type_naming).unwrap();
        assert_eq!(fields, decoded);
    }
}
