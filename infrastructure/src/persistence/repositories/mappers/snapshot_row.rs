//! Row mappers for `DocumentSnapshot` and `SnapshotHeader`.

use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use domain::content::header::{DocumentSnapshot, SnapshotHeader, SnapshotId};
use domain::content::instance::DocumentInstanceId;
use domain::schema::DocumentTypeId;

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{
    COL_CONTENT, COL_CREATED_AT, COL_CREATED_BY, COL_DOCUMENT_ID, COL_DOCUMENT_TYPE_ID,
    COL_ID, COL_REASON, COL_RELATIONS, COL_REVISION, DocumentTypeNaming,
};
use crate::persistence::repositories::codec::{
    deserialize_fields_json, deserialize_relations_json, parse_actor_urn,
    snapshot_reason_from_db_str,
};

// ---------------------------------------------------------------------------
// Public Functions
// ---------------------------------------------------------------------------

/// Maps a row from `document_snapshots` into a lightweight [`SnapshotHeader`].
pub fn map_snapshot_header_row(row: &PgRow) -> Result<SnapshotHeader, PersistenceError> {
    let id: Uuid = row.try_get(COL_ID)?;
    let type_id_str: String = row.try_get(COL_DOCUMENT_TYPE_ID)?;
    let document_type_id = DocumentTypeId::try_new(type_id_str)
        .map_err(|e| PersistenceError::Validation(e.to_string()))?;
    let doc_id: Uuid = row.try_get(COL_DOCUMENT_ID)?;
    let revision_i32: i32 = row.try_get(COL_REVISION)?;
    let revision = u32::try_from(revision_i32).map_err(|e| {
        PersistenceError::Validation(format!("invalid negative revision '{revision_i32}': {e}"))
    })?;
    let created_at: DateTime<Utc> = row.try_get(COL_CREATED_AT)?;
    let created_by_str: String = row.try_get(COL_CREATED_BY)?;
    let created_by = parse_actor_urn(&created_by_str)?;
    let reason_str: String = row.try_get(COL_REASON)?;
    let reason = snapshot_reason_from_db_str(&reason_str)?;

    Ok(SnapshotHeader {
        id: SnapshotId::new(id),
        document_type_id,
        document_id: DocumentInstanceId::new(doc_id),
        revision,
        created_at,
        created_by,
        reason,
    })
}

/// Maps a row from `document_snapshots` into a fully deserialized [`DocumentSnapshot`].
pub fn map_snapshot_row(
    row: &PgRow,
    type_naming: &DocumentTypeNaming,
) -> Result<DocumentSnapshot, PersistenceError> {
    let header = map_snapshot_header_row(row)?;
    let content_val: serde_json::Value = row.try_get(COL_CONTENT)?;
    let fields = deserialize_fields_json(content_val, type_naming)?;
    let rel_val: serde_json::Value = row.try_get(COL_RELATIONS)?;
    let relations = deserialize_relations_json(rel_val)?;

    Ok(DocumentSnapshot {
        id: header.id,
        document_type_id: header.document_type_id,
        document_id: header.document_id,
        revision: header.revision,
        fields,
        relations,
        created_at: header.created_at,
        created_by: header.created_by,
        reason: header.reason,
    })
}
