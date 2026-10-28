//! Sea-Query dynamic query builders for the `document_snapshots` revision history table.

use sea_query::{Expr, ExprTrait, InsertStatement, Order, Query, SelectStatement};
use uuid::Uuid;

use domain::content::header::DocumentSnapshot;
use domain::schema::DocumentTypeId;

use crate::persistence::naming::StaticIden;
use super::common::{
    COL_CONTENT_IDEN, COL_CREATED_AT_IDEN, COL_CREATED_BY_IDEN, COL_DOCUMENT_ID_IDEN,
    COL_DOCUMENT_TYPE_ID_IDEN, COL_ID_IDEN, COL_REASON_IDEN, COL_RELATIONS_IDEN,
    COL_REVISION_IDEN, TABLE_DOCUMENT_SNAPSHOTS_IDEN,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SNAPSHOT_INSERT_COLUMNS: [StaticIden; 9] = [
    COL_ID_IDEN,
    COL_DOCUMENT_TYPE_ID_IDEN,
    COL_DOCUMENT_ID_IDEN,
    COL_REVISION_IDEN,
    COL_CONTENT_IDEN,
    COL_RELATIONS_IDEN,
    COL_CREATED_AT_IDEN,
    COL_CREATED_BY_IDEN,
    COL_REASON_IDEN,
];

const SNAPSHOT_HEADER_COLUMNS: [StaticIden; 7] = [
    COL_ID_IDEN,
    COL_DOCUMENT_TYPE_ID_IDEN,
    COL_DOCUMENT_ID_IDEN,
    COL_REVISION_IDEN,
    COL_CREATED_AT_IDEN,
    COL_CREATED_BY_IDEN,
    COL_REASON_IDEN,
];

const SNAPSHOT_FULL_COLUMNS: [StaticIden; 9] = [
    COL_ID_IDEN,
    COL_DOCUMENT_TYPE_ID_IDEN,
    COL_DOCUMENT_ID_IDEN,
    COL_REVISION_IDEN,
    COL_CONTENT_IDEN,
    COL_RELATIONS_IDEN,
    COL_CREATED_AT_IDEN,
    COL_CREATED_BY_IDEN,
    COL_REASON_IDEN,
];

// ---------------------------------------------------------------------------
// Public Query Builders
// ---------------------------------------------------------------------------

/// Builds an `INSERT INTO document_snapshots (...) VALUES (...)` statement.
#[must_use]
pub fn insert_snapshot_query(
    snapshot: &DocumentSnapshot,
    content_json: serde_json::Value,
    relations_json: serde_json::Value,
    reason_str: &'static str,
) -> InsertStatement {
    let rev_i32 = i32::try_from(snapshot.revision).unwrap_or(i32::MAX);
    let mut stmt = Query::insert();
    stmt.into_table(TABLE_DOCUMENT_SNAPSHOTS_IDEN)
        .columns(SNAPSHOT_INSERT_COLUMNS)
        .values_panic([
            (*snapshot.id).into(),
            snapshot.document_type_id.as_ref().into(),
            (*snapshot.document_id).into(),
            rev_i32.into(),
            content_json.into(),
            relations_json.into(),
            snapshot.created_at.into(),
            snapshot.created_by.to_urn().into(),
            reason_str.into(),
        ]);
    stmt
}

/// Builds a `SELECT ... FROM document_snapshots WHERE document_type_id = $1 AND document_id = $2 ORDER BY revision DESC` query.
#[must_use]
pub fn list_snapshot_headers_query(type_id: &DocumentTypeId, doc_id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_DOCUMENT_SNAPSHOTS_IDEN)
        .columns(SNAPSHOT_HEADER_COLUMNS)
        .and_where(Expr::col(COL_DOCUMENT_TYPE_ID_IDEN).eq(type_id.as_ref()))
        .and_where(Expr::col(COL_DOCUMENT_ID_IDEN).eq(doc_id))
        .order_by(COL_REVISION_IDEN, Order::Desc);
    stmt
}

/// Builds a query to fetch a single historical snapshot by exact revision.
#[must_use]
pub fn get_snapshot_by_revision_query(
    type_id: &DocumentTypeId,
    doc_id: Uuid,
    revision: u32,
) -> SelectStatement {
    let rev_i32 = i32::try_from(revision).unwrap_or(i32::MAX);
    let mut stmt = Query::select();
    stmt.from(TABLE_DOCUMENT_SNAPSHOTS_IDEN)
        .columns(SNAPSHOT_FULL_COLUMNS)
        .and_where(Expr::col(COL_DOCUMENT_TYPE_ID_IDEN).eq(type_id.as_ref()))
        .and_where(Expr::col(COL_DOCUMENT_ID_IDEN).eq(doc_id))
        .and_where(Expr::col(COL_REVISION_IDEN).eq(rev_i32));
    stmt
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use chrono::Utc;
    use sea_query::PostgresQueryBuilder;
    use serde_json::json;

    use domain::auth::ActorId;
    use domain::content::header::{SnapshotId, SnapshotReason};
    use domain::content::instance::DocumentInstanceId;
    use crate::persistence::query::common::{compile_insert, compile_select};

    fn make_test_snapshot() -> DocumentSnapshot {
        DocumentSnapshot {
            id: SnapshotId::new(Uuid::now_v7()),
            document_type_id: DocumentTypeId::try_new("article").unwrap(),
            document_id: DocumentInstanceId::new(Uuid::now_v7()),
            revision: 3,
            fields: HashMap::new(),
            relations: HashMap::new(),
            created_at: Utc::now(),
            created_by: ActorId::System,
            reason: SnapshotReason::Publish,
        }
    }

    #[test]
    fn test_insert_snapshot_query() {
        let snapshot = make_test_snapshot();
        let stmt = insert_snapshot_query(
            &snapshot,
            json!({"title": "Archived Title"}),
            json!({}),
            "publish",
        );

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("INSERT INTO \"document_snapshots\""));
        assert!(unparam.contains("\"revision\", \"content\", \"relations\""));
        assert!(unparam.contains("'publish'"));

        let (sql, _) = compile_insert(&stmt);
        assert!(sql.contains("INSERT INTO \"document_snapshots\""));
    }

    #[test]
    fn test_list_snapshot_headers_query() {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let doc_id = Uuid::now_v7();
        let stmt = list_snapshot_headers_query(&type_id, doc_id);

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("FROM \"document_snapshots\""));
        assert!(unparam.contains("\"document_type_id\" = 'article'"));
        assert!(unparam.contains("ORDER BY \"revision\" DESC"));

        let (sql, _) = compile_select(&stmt);
        assert!(sql.contains("WHERE \"document_type_id\" = $1 AND \"document_id\" = $2"));
        assert!(sql.contains("ORDER BY \"revision\" DESC"));
    }

    #[test]
    fn test_get_snapshot_by_revision_query() {
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let doc_id = Uuid::now_v7();
        let stmt = get_snapshot_by_revision_query(&type_id, doc_id, 5);

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("FROM \"document_snapshots\""));
        assert!(unparam.contains("\"revision\" = 5"));

        let (sql, _) = compile_select(&stmt);
        assert!(sql.contains("WHERE \"document_type_id\" = $1 AND \"document_id\" = $2 AND \"revision\" = $3"));
    }
}
