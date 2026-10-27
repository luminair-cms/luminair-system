//! Row mappers for `DocumentInstance` and `DocumentHeader`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use domain::auth::user::ActorId;
use domain::common::{Email, Url};
use domain::content::header::DocumentHeader;
use domain::content::instance::{
    AuditTrail, DocumentContent, DocumentInstance, DocumentInstanceId, PublicationState,
    ResolvedRelation,
};
use domain::content::values::{ContentValue, DomainValue, PrimitiveValue};
use domain::schema::{AttributeId, DocumentTypeId, FieldType, IntegerSize, PrimitiveType};

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{
    COL_CONTENT, COL_CREATED_AT, COL_CREATED_BY, COL_DISPLAY_TITLE, COL_DISPLAY_VALUES,
    COL_ID, COL_LAST_PUBLISHED_REVISION, COL_PUBLISHED_AT, COL_PUBLISHED_BY, COL_RELATIONS,
    COL_REVISION, COL_STATUS, COL_UPDATED_AT, COL_UPDATED_BY, COL_VERSION, DocumentTypeNaming,
    FieldColumnNaming,
};
use crate::persistence::repositories::codec::{
    deserialize_display_values_json, deserialize_fields_json, deserialize_json_map,
    deserialize_localized_text, deserialize_relations_json, parse_actor_urn,
    parse_optional_actor_urn, status_from_db_str,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const COL_SLUG: &str = "slug";
const COL_DRAFT_VERSION: &str = "draft_version";

// ---------------------------------------------------------------------------
// Public Functions
// ---------------------------------------------------------------------------

/// Reconstitutes a published [`DocumentInstance`] from a wide-table [`PgRow`].
pub fn map_wide_table_row(
    row: &PgRow,
    type_naming: &DocumentTypeNaming,
    relations: HashMap<AttributeId, Vec<ResolvedRelation>>,
) -> Result<DocumentInstance, PersistenceError> {
    let id = DocumentInstanceId::new(row.try_get(COL_ID)?);
    let rev_i32: i32 = row.try_get(COL_REVISION)?;
    let revision = u32::try_from(rev_i32).map_err(|e| {
        PersistenceError::Validation(format!("invalid negative revision '{rev_i32}': {e}"))
    })?;
    let published_at: DateTime<Utc> = row.try_get(COL_PUBLISHED_AT)?;
    let published_by = parse_actor_urn(row.try_get::<&str, _>(COL_PUBLISHED_BY)?)?;
    let created_at: DateTime<Utc> = row.try_get(COL_CREATED_AT)?;
    let created_by = parse_actor_urn(row.try_get::<&str, _>(COL_CREATED_BY)?)?;
    let updated_at: DateTime<Utc> = row.try_get(COL_UPDATED_AT)?;
    let updated_by = parse_actor_urn(row.try_get::<&str, _>(COL_UPDATED_BY)?)?;

    let mut fields = HashMap::with_capacity(type_naming.field_columns.len());
    for (attr_id, col_naming) in &type_naming.field_columns {
        if let Some(val) = extract_custom_field(row, col_naming, attr_id)? {
            fields.insert(attr_id.clone(), val);
        }
    }

    let publication_state = PublicationState::Published {
        revision,
        published_at,
        published_by,
    };
    let audit = AuditTrail {
        created_at,
        created_by,
        updated_at,
        updated_by,
        version: revision,
    };

    Ok(DocumentInstance::reconstitute(
        id,
        type_naming.document_type_id.clone(),
        DocumentContent::new(fields, publication_state),
        relations,
        audit,
    ))
}

/// Reconstitutes a working [`DocumentInstance`] from a `document_drafts` [`PgRow`].
pub fn map_draft_row(
    row: &PgRow,
    type_naming: &DocumentTypeNaming,
    live_published_meta: Option<(u32, DateTime<Utc>, ActorId)>,
) -> Result<DocumentInstance, PersistenceError> {
    let id = DocumentInstanceId::new(row.try_get(COL_ID)?);
    let status_str: &str = row.try_get(COL_STATUS)?;
    let content_val: serde_json::Value = row.try_get(COL_CONTENT)?;
    let fields = deserialize_fields_json(content_val, type_naming)?;
    let rel_val: serde_json::Value = row.try_get(COL_RELATIONS)?;
    let relations = deserialize_relations_json(rel_val)?;

    let version_i32: i32 = row.try_get(COL_VERSION)?;
    let version = u32::try_from(version_i32).map_err(|e| {
        PersistenceError::Validation(format!("invalid negative draft version '{version_i32}': {e}"))
    })?;
    let last_published_rev = row
        .try_get::<Option<i32>, _>(COL_LAST_PUBLISHED_REVISION)?
        .map(|r| u32::try_from(r).unwrap_or(0));

    let created_at: DateTime<Utc> = row.try_get(COL_CREATED_AT)?;
    let created_by = parse_actor_urn(row.try_get::<&str, _>(COL_CREATED_BY)?)?;
    let updated_at: DateTime<Utc> = row.try_get(COL_UPDATED_AT)?;
    let updated_by = parse_actor_urn(row.try_get::<&str, _>(COL_UPDATED_BY)?)?;

    let publication_state = resolve_draft_publication_state(
        status_str,
        version,
        last_published_rev,
        updated_at,
        &updated_by,
        live_published_meta,
    )?;

    let audit = AuditTrail {
        created_at,
        created_by,
        updated_at,
        updated_by,
        version,
    };

    Ok(DocumentInstance::reconstitute(
        id,
        type_naming.document_type_id.clone(),
        DocumentContent::new(fields, publication_state),
        relations,
        audit,
    ))
}

/// Extracts `(revision, published_at, published_by)` from a wide-table [`PgRow`].
pub fn extract_wide_published_meta(
    row: &PgRow,
) -> Result<(u32, DateTime<Utc>, ActorId), PersistenceError> {
    let rev_i32: i32 = row.try_get(COL_REVISION)?;
    let revision = u32::try_from(rev_i32).map_err(|e| {
        PersistenceError::Validation(format!("invalid negative revision '{rev_i32}': {e}"))
    })?;
    let published_at: DateTime<Utc> = row.try_get(COL_PUBLISHED_AT)?;
    let published_by_str: &str = row.try_get(COL_PUBLISHED_BY)?;
    let published_by = parse_actor_urn(published_by_str)?;
    Ok((revision, published_at, published_by))
}

/// Maps a header query [`PgRow`] into a [`DocumentHeader`].
pub fn map_header_row(
    row: &PgRow,
    type_id: &DocumentTypeId,
) -> Result<DocumentHeader, PersistenceError> {
    let id = DocumentInstanceId::new(row.try_get(COL_ID)?);
    let display_values_val: serde_json::Value = row.try_get(COL_DISPLAY_VALUES)?;
    let display_values = deserialize_display_values_json(display_values_val)?;
    let display_title: Option<String> = row.try_get(COL_DISPLAY_TITLE)?;
    let status_str: &str = row.try_get(COL_STATUS)?;
    let status = status_from_db_str(status_str)?;
    let slug: Option<String> = row.try_get(COL_SLUG)?;

    let revision = row
        .try_get::<Option<i32>, _>(COL_REVISION)?
        .map(|r| u32::try_from(r).unwrap_or(0));
    let draft_version = row
        .try_get::<Option<i32>, _>(COL_DRAFT_VERSION)?
        .map(|v| u32::try_from(v).unwrap_or(0));

    let created_at: DateTime<Utc> = row.try_get(COL_CREATED_AT)?;
    let created_by = parse_actor_urn(row.try_get::<&str, _>(COL_CREATED_BY)?)?;
    let updated_at: DateTime<Utc> = row.try_get(COL_UPDATED_AT)?;
    let updated_by = parse_actor_urn(row.try_get::<&str, _>(COL_UPDATED_BY)?)?;
    let published_at: Option<DateTime<Utc>> = row.try_get(COL_PUBLISHED_AT)?;
    let published_by =
        parse_optional_actor_urn(row.try_get::<Option<&str>, _>(COL_PUBLISHED_BY)?)?;

    Ok(DocumentHeader {
        id,
        document_type_id: type_id.clone(),
        display_values,
        display_title: display_title.unwrap_or_default(),
        relational_display_target: None,
        status,
        slug,
        revision,
        draft_version,
        created_at,
        created_by,
        updated_at,
        updated_by,
        published_at,
        published_by,
    })
}

/// Maps a row containing `id` and `display_title` into a `(DocumentInstanceId, String)` tuple.
pub fn map_display_title_row(
    row: &PgRow,
) -> Result<(DocumentInstanceId, String), PersistenceError> {
    let id: Uuid = row.try_get(COL_ID)?;
    let display_title: Option<String> = row.try_get(COL_DISPLAY_TITLE)?;
    Ok((DocumentInstanceId::new(id), display_title.unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn extract_custom_field(
    row: &PgRow,
    col: &FieldColumnNaming,
    attr_id: &AttributeId,
) -> Result<Option<ContentValue>, PersistenceError> {
    match col.field_type {
        FieldType::Primitive(pt) => extract_primitive_field(row, col.column_name, pt),
        FieldType::Email => {
            let opt: Option<String> = row.try_get(col.column_name)?;
            opt.map(|s| {
                Email::try_new(s)
                    .map(|e| ContentValue::Scalar(DomainValue::Email(e)))
                    .map_err(|e| PersistenceError::Validation(e.to_string()))
            })
            .transpose()
        }
        FieldType::Url => {
            let opt: Option<String> = row.try_get(col.column_name)?;
            opt.map(|s| {
                Url::try_new(s)
                    .map(|u| ContentValue::Scalar(DomainValue::Url(u)))
                    .map_err(|e| PersistenceError::Validation(e.to_string()))
            })
            .transpose()
        }
        FieldType::Json => {
            let opt: Option<serde_json::Value> = row.try_get(col.column_name)?;
            opt.map(|val| deserialize_json_map(val, attr_id)).transpose()
        }
        FieldType::LocalizedText => {
            let opt: Option<serde_json::Value> = row.try_get(col.column_name)?;
            opt.map(|val| deserialize_localized_text(val, attr_id))
                .transpose()
        }
    }
}

fn extract_primitive_field(
    row: &PgRow,
    col_name: &'static str,
    pt: PrimitiveType,
) -> Result<Option<ContentValue>, PersistenceError> {
    match pt {
        PrimitiveType::Text => {
            let opt: Option<String> = row.try_get(col_name)?;
            Ok(opt.map(|s| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(s)))))
        }
        PrimitiveType::Uid => {
            let opt: Option<String> = row.try_get(col_name)?;
            Ok(opt.map(|s| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Uid(s)))))
        }
        PrimitiveType::Uuid => {
            let opt: Option<Uuid> = row.try_get(col_name)?;
            Ok(opt.map(|u| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Uuid(u)))))
        }
        PrimitiveType::Integer(IntegerSize::I16) => {
            let opt: Option<i16> = row.try_get(col_name)?;
            Ok(opt.map(|i| {
                ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(i64::from(i))))
            }))
        }
        PrimitiveType::Integer(IntegerSize::I32) => {
            let opt: Option<i32> = row.try_get(col_name)?;
            Ok(opt.map(|i| {
                ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(i64::from(i))))
            }))
        }
        PrimitiveType::Integer(IntegerSize::I64) => {
            let opt: Option<i64> = row.try_get(col_name)?;
            Ok(opt.map(|i| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(i)))))
        }
        PrimitiveType::Decimal { .. } => {
            let opt: Option<rust_decimal::Decimal> = row.try_get(col_name)?;
            Ok(opt.map(|d| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Decimal(d)))))
        }
        PrimitiveType::Date => {
            let opt: Option<chrono::NaiveDate> = row.try_get(col_name)?;
            Ok(opt.map(|d| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Date(d)))))
        }
        PrimitiveType::DateTime => {
            let opt: Option<DateTime<Utc>> = row.try_get(col_name)?;
            Ok(opt.map(|dt| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::DateTime(dt)))))
        }
        PrimitiveType::Boolean => {
            let opt: Option<bool> = row.try_get(col_name)?;
            Ok(opt.map(|b| ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Boolean(b)))))
        }
    }
}

fn resolve_draft_publication_state(
    status: &str,
    version: u32,
    last_published_rev: Option<u32>,
    updated_at: DateTime<Utc>,
    updated_by: &ActorId,
    live_published_meta: Option<(u32, DateTime<Utc>, ActorId)>,
) -> Result<PublicationState, PersistenceError> {
    match status {
        "draft" => Ok(PublicationState::Draft),
        "modified" => {
            if let Some((live_rev, pub_at, pub_by)) = live_published_meta {
                Ok(PublicationState::Modified {
                    live_revision: live_rev,
                    published_at: pub_at,
                    published_by: pub_by,
                    draft_version: version,
                })
            } else {
                let live_rev = last_published_rev.unwrap_or(1);
                Ok(PublicationState::Modified {
                    live_revision: live_rev,
                    published_at: updated_at,
                    published_by: updated_by.clone(),
                    draft_version: version,
                })
            }
        }
        "unpublished" => {
            let last_rev = last_published_rev.unwrap_or(1);
            Ok(PublicationState::Unpublished {
                last_published_revision: last_rev,
                unpublished_at: updated_at,
                unpublished_by: updated_by.clone(),
            })
        }
        unknown => Err(PersistenceError::Validation(format!(
            "unexpected draft row status '{unknown}'"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_draft_publication_state() {
        let now = Utc::now();
        let user = ActorId::System;

        let st1 = resolve_draft_publication_state("draft", 1, None, now, &user, None).unwrap();
        assert_eq!(st1, PublicationState::Draft);

        let live_meta = Some((3, now, user.clone()));
        let st2 = resolve_draft_publication_state("modified", 4, Some(3), now, &user, live_meta).unwrap();
        assert_eq!(
            st2,
            PublicationState::Modified {
                live_revision: 3,
                published_at: now,
                published_by: user.clone(),
                draft_version: 4,
            }
        );

        let st3 = resolve_draft_publication_state("unpublished", 4, Some(3), now, &user, None).unwrap();
        assert_eq!(
            st3,
            PublicationState::Unpublished {
                last_published_revision: 3,
                unpublished_at: now,
                unpublished_by: user,
            }
        );

        let err = resolve_draft_publication_state("invalid", 1, None, now, &ActorId::System, None);
        assert!(err.is_err());
    }
}
