//! Sea-Query dynamic query builders for the unified `document_drafts` table.

use chrono::{DateTime, Utc};
use sea_query::{
    Asterisk, DeleteStatement, Expr, ExprTrait, InsertStatement, OnConflict, Order, Query,
    SelectStatement,
};
use uuid::Uuid;

use domain::schema::DocumentTypeId;

use crate::persistence::naming::StaticIden;
use super::common::{
    IDEN_CONTENT, IDEN_CREATED_AT, IDEN_CREATED_BY, IDEN_DISPLAY_TITLE, IDEN_DISPLAY_VALUES,
    IDEN_DOCUMENT_TYPE_ID, IDEN_ID, IDEN_LAST_PUBLISHED_REVISION, IDEN_RELATIONS,
    IDEN_SCHEMA_VERSION, IDEN_STATUS, IDEN_UPDATED_AT, IDEN_UPDATED_BY, IDEN_VERSION,
    TABLE_DOCUMENT_DRAFTS_IDEN,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DRAFT_INSERT_COLUMNS: [StaticIden; 14] = [
    IDEN_ID,
    IDEN_DOCUMENT_TYPE_ID,
    IDEN_DISPLAY_VALUES,
    IDEN_DISPLAY_TITLE,
    IDEN_STATUS,
    IDEN_CONTENT,
    IDEN_RELATIONS,
    IDEN_SCHEMA_VERSION,
    IDEN_VERSION,
    IDEN_LAST_PUBLISHED_REVISION,
    IDEN_CREATED_AT,
    IDEN_CREATED_BY,
    IDEN_UPDATED_AT,
    IDEN_UPDATED_BY,
];

const DRAFT_UPDATE_COLUMNS: [StaticIden; 10] = [
    IDEN_DISPLAY_VALUES,
    IDEN_DISPLAY_TITLE,
    IDEN_STATUS,
    IDEN_CONTENT,
    IDEN_RELATIONS,
    IDEN_SCHEMA_VERSION,
    IDEN_VERSION,
    IDEN_LAST_PUBLISHED_REVISION,
    IDEN_UPDATED_AT,
    IDEN_UPDATED_BY,
];

// ---------------------------------------------------------------------------
// Public Types
// ---------------------------------------------------------------------------

/// Parameters for upserting a document working draft into `document_drafts`.
#[derive(Debug, Clone)]
pub struct DraftUpsertParams<'a> {
    pub id: Uuid,
    pub document_type_id: &'a DocumentTypeId,
    pub display_values_json: serde_json::Value,
    pub display_title: &'a str,
    pub status: &'static str,
    pub content_json: serde_json::Value,
    pub relations_json: serde_json::Value,
    pub schema_version: i32,
    pub version: i32,
    pub last_published_revision: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub created_by_urn: String,
    pub updated_at: DateTime<Utc>,
    pub updated_by_urn: String,
}

// ---------------------------------------------------------------------------
// Public Query Builders
// ---------------------------------------------------------------------------

/// Builds a `SELECT * FROM document_drafts WHERE document_type_id = $1 AND id = $2` query.
#[must_use]
pub fn select_draft_by_id_query(type_id: &DocumentTypeId, id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_DOCUMENT_DRAFTS_IDEN)
        .column(Asterisk)
        .and_where(Expr::col(IDEN_DOCUMENT_TYPE_ID).eq(type_id.as_ref()))
        .and_where(Expr::col(IDEN_ID).eq(id));
    stmt
}

/// Builds a `SELECT * FROM document_drafts WHERE document_type_id = $1 ORDER BY updated_at DESC` query.
#[must_use]
pub fn select_drafts_by_type_query(type_id: &DocumentTypeId) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_DOCUMENT_DRAFTS_IDEN)
        .column(Asterisk)
        .and_where(Expr::col(IDEN_DOCUMENT_TYPE_ID).eq(type_id.as_ref()))
        .order_by(IDEN_UPDATED_AT, Order::Desc);
    stmt
}

/// Builds an upsert statement into `document_drafts` with `ON CONFLICT (document_type_id, id) DO UPDATE`.
#[must_use]
pub fn upsert_draft_query(params: DraftUpsertParams<'_>) -> InsertStatement {
    let mut stmt = Query::insert();
    stmt.into_table(TABLE_DOCUMENT_DRAFTS_IDEN)
        .columns(DRAFT_INSERT_COLUMNS)
        .values_panic([
            params.id.into(),
            params.document_type_id.as_ref().into(),
            params.display_values_json.into(),
            params.display_title.into(),
            params.status.into(),
            params.content_json.into(),
            params.relations_json.into(),
            params.schema_version.into(),
            params.version.into(),
            params.last_published_revision.into(),
            params.created_at.into(),
            params.created_by_urn.into(),
            params.updated_at.into(),
            params.updated_by_urn.into(),
        ])
        .on_conflict(
            OnConflict::columns([IDEN_DOCUMENT_TYPE_ID, IDEN_ID])
                .update_columns(DRAFT_UPDATE_COLUMNS)
                .to_owned(),
        );
    stmt
}

/// Builds a `DELETE FROM document_drafts WHERE document_type_id = $1 AND id = $2` query.
#[must_use]
pub fn delete_draft_query(type_id: &DocumentTypeId, id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(TABLE_DOCUMENT_DRAFTS_IDEN)
        .and_where(Expr::col(IDEN_DOCUMENT_TYPE_ID).eq(type_id.as_ref()))
        .and_where(Expr::col(IDEN_ID).eq(id));
    stmt
}

/// Builds a `SELECT 1 FROM document_drafts WHERE document_type_id = $1 LIMIT 1` existence probe query.
#[must_use]
pub fn exists_draft_for_type_query(type_id: &DocumentTypeId) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(TABLE_DOCUMENT_DRAFTS_IDEN)
        .expr(Expr::val(1))
        .and_where(Expr::col(IDEN_DOCUMENT_TYPE_ID).eq(type_id.as_ref()))
        .limit(1);
    stmt
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::PostgresQueryBuilder;
    use serde_json::json;

    use crate::persistence::query::common::{compile_delete, compile_insert, compile_select};

    fn sample_doc_type_id() -> DocumentTypeId {
        DocumentTypeId::try_new("article").unwrap()
    }

    #[test]
    fn test_select_draft_by_id_query() {
        let type_id = sample_doc_type_id();
        let id = Uuid::now_v7();
        let stmt = select_draft_by_id_query(&type_id, id);

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("FROM \"document_drafts\""));
        assert!(unparam.contains("\"document_type_id\" = 'article'"));

        let (sql, _) = compile_select(&stmt);
        assert!(sql.contains("WHERE \"document_type_id\" = $1 AND \"id\" = $2"));
    }

    #[test]
    fn test_select_drafts_by_type_query() {
        let type_id = sample_doc_type_id();
        let stmt = select_drafts_by_type_query(&type_id);

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("FROM \"document_drafts\""));
        assert!(unparam.contains("ORDER BY \"updated_at\" DESC"));

        let (sql, _) = compile_select(&stmt);
        assert!(sql.contains("WHERE \"document_type_id\" = $1"));
        assert!(sql.contains("ORDER BY \"updated_at\" DESC"));
    }

    #[test]
    fn test_upsert_draft_query() {
        let type_id = sample_doc_type_id();
        let id = Uuid::now_v7();
        let now = Utc::now();
        let params = DraftUpsertParams {
            id,
            document_type_id: &type_id,
            display_values_json: json!({"title": "Draft Title"}),
            display_title: "Draft Title",
            status: "draft",
            content_json: json!({"body": "Draft content"}),
            relations_json: json!({}),
            schema_version: 1,
            version: 1,
            last_published_revision: None,
            created_at: now,
            created_by_urn: "user:test".to_string(),
            updated_at: now,
            updated_by_urn: "user:test".to_string(),
        };

        let stmt = upsert_draft_query(params);
        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("INSERT INTO \"document_drafts\""));
        assert!(unparam.contains("ON CONFLICT (\"document_type_id\", \"id\") DO UPDATE SET"));
        assert!(unparam.contains("\"display_title\" = \"excluded\".\"display_title\""));
        assert!(unparam.contains("\"status\" = \"excluded\".\"status\""));

        let (sql, _) = compile_insert(&stmt);
        assert!(sql.contains("INSERT INTO \"document_drafts\""));
        assert!(sql.contains("ON CONFLICT (\"document_type_id\", \"id\") DO UPDATE SET"));
    }

    #[test]
    fn test_delete_draft_query() {
        let type_id = sample_doc_type_id();
        let id = Uuid::now_v7();
        let stmt = delete_draft_query(&type_id, id);

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("DELETE FROM \"document_drafts\""));

        let (sql, _) = compile_delete(&stmt);
        assert!(sql.contains("WHERE \"document_type_id\" = $1 AND \"id\" = $2"));
    }

    #[test]
    fn test_exists_draft_for_type_query() {
        let type_id = sample_doc_type_id();
        let stmt = exists_draft_for_type_query(&type_id);

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("SELECT 1 FROM \"document_drafts\""));
        assert!(unparam.contains("LIMIT 1"));

        let (sql, _) = compile_select(&stmt);
        assert!(sql.contains("WHERE \"document_type_id\" = $2"));
        assert!(sql.contains("LIMIT $3"));
    }
}
