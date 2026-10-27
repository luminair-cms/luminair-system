//! Sea-Query dynamic query builders for Admin Studio document headers and display title batch enrichment.

use sea_query::{
    Asterisk, Expr, ExprTrait, Func, Order, Query, SelectStatement, UnionType,
};
use uuid::Uuid;

use domain::content::header::DocumentStatus;
use domain::content::ports::Pagination;

use crate::persistence::naming::{DocumentTypeNaming, StaticIden};
use super::common::{
    IDEN_CREATED_AT, IDEN_CREATED_BY, IDEN_DISPLAY_TITLE, IDEN_DISPLAY_VALUES,
    IDEN_DOCUMENT_TYPE_ID, IDEN_HAS_PENDING_DRAFT, IDEN_ID, IDEN_LAST_PUBLISHED_REVISION,
    IDEN_PUBLISHED_AT, IDEN_PUBLISHED_BY, IDEN_REVISION, IDEN_STATUS, IDEN_UPDATED_AT,
    IDEN_UPDATED_BY, IDEN_VERSION, TABLE_DOCUMENT_DRAFTS_IDEN, apply_pagination,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const IDEN_SLUG: StaticIden = StaticIden("slug");
const IDEN_DRAFT_VERSION: StaticIden = StaticIden("draft_version");
const IDEN_COMBINED: StaticIden = StaticIden("combined");

// ---------------------------------------------------------------------------
// Public Functions
// ---------------------------------------------------------------------------

/// Builds a paginated `list_admin_headers` query for single-lifecycle (`draft_and_publish: false`) types.
#[must_use]
pub fn list_single_lifecycle_headers_query(
    type_naming: &DocumentTypeNaming,
    pagination: Pagination,
    status_filter: Option<DocumentStatus>,
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(type_naming.table_iden);
    select_wide_table_header_projection(&mut stmt, type_naming);

    if status_filter.is_some_and(|st| st != DocumentStatus::Published) {
        stmt.and_where(Expr::val(true).eq(false));
    }

    stmt.order_by(IDEN_UPDATED_AT, Order::Desc)
        .order_by(IDEN_ID, Order::Asc);
    apply_pagination(&mut stmt, pagination);
    stmt
}

/// Builds a `COUNT(*)` query for single-lifecycle (`draft_and_publish: false`) types.
#[must_use]
pub fn count_single_lifecycle_headers_query(
    type_naming: &DocumentTypeNaming,
    status_filter: Option<DocumentStatus>,
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(type_naming.table_iden)
        .expr(Func::count(Expr::col(IDEN_ID)));

    if status_filter.is_some_and(|st| st != DocumentStatus::Published) {
        stmt.and_where(Expr::val(true).eq(false));
    }

    stmt
}

/// Builds a paginated `list_admin_headers` two-branch `UNION ALL` query for draft-enabled types.
#[must_use]
pub fn list_draft_enabled_headers_query(
    type_naming: &DocumentTypeNaming,
    pagination: Pagination,
    status_filter: Option<DocumentStatus>,
) -> SelectStatement {
    let union_query = build_draft_enabled_union_query(type_naming);

    let mut outer = Query::select();
    outer
        .from_subquery(union_query, IDEN_COMBINED)
        .column(Asterisk);

    if let Some(status) = status_filter {
        outer.and_where(Expr::col(IDEN_STATUS).eq(document_status_str(status)));
    }

    outer
        .order_by(IDEN_UPDATED_AT, Order::Desc)
        .order_by(IDEN_ID, Order::Asc);
    apply_pagination(&mut outer, pagination);
    outer
}

/// Builds a `COUNT(*)` query over the combined draft-enabled `UNION ALL` subquery.
#[must_use]
pub fn count_draft_enabled_headers_query(
    type_naming: &DocumentTypeNaming,
    status_filter: Option<DocumentStatus>,
) -> SelectStatement {
    let union_query = build_draft_enabled_union_query(type_naming);

    let mut outer = Query::select();
    outer
        .from_subquery(union_query, IDEN_COMBINED)
        .expr(Func::count(Expr::col(IDEN_ID)));

    if let Some(status) = status_filter {
        outer.and_where(Expr::col(IDEN_STATUS).eq(document_status_str(status)));
    }

    outer
}

/// Builds a batched query to retrieve `id` and `display_title` for relation target enrichment.
#[must_use]
pub fn find_display_titles_by_ids_query(
    type_naming: &DocumentTypeNaming,
    ids: &[Uuid],
    draft_and_publish: bool,
) -> SelectStatement {
    if !draft_and_publish {
        let mut stmt = Query::select();
        stmt.from(type_naming.table_iden)
            .columns([IDEN_ID, IDEN_DISPLAY_TITLE])
            .and_where(Expr::col(IDEN_ID).is_in(ids.to_vec()));
        return stmt;
    }

    let mut b1 = Query::select();
    b1.from(type_naming.table_iden)
        .columns([IDEN_ID, IDEN_DISPLAY_TITLE])
        .and_where(Expr::col(IDEN_HAS_PENDING_DRAFT).eq(false))
        .and_where(Expr::col(IDEN_ID).is_in(ids.to_vec()));

    let mut b2 = Query::select();
    b2.from(TABLE_DOCUMENT_DRAFTS_IDEN)
        .columns([IDEN_ID, IDEN_DISPLAY_TITLE])
        .and_where(Expr::col(IDEN_DOCUMENT_TYPE_ID).eq(type_naming.document_type_id.as_ref()))
        .and_where(Expr::col(IDEN_ID).is_in(ids.to_vec()));

    b1.union(UnionType::All, b2);
    b1
}

/// Returns the wire status string matching a `DocumentStatus` variant.
#[must_use]
pub fn document_status_str(status: DocumentStatus) -> &'static str {
    match status {
        DocumentStatus::Draft => "draft",
        DocumentStatus::Published => "published",
        DocumentStatus::Modified => "modified",
        DocumentStatus::Unpublished => "unpublished",
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn build_draft_enabled_union_query(type_naming: &DocumentTypeNaming) -> SelectStatement {
    let mut b1 = Query::select();
    b1.from(TABLE_DOCUMENT_DRAFTS_IDEN);
    select_draft_header_projection(&mut b1);
    b1.and_where(Expr::col(IDEN_DOCUMENT_TYPE_ID).eq(type_naming.document_type_id.as_ref()));

    let mut b2 = Query::select();
    b2.from(type_naming.table_iden);
    select_wide_table_header_projection(&mut b2, type_naming);
    b2.and_where(Expr::col(IDEN_HAS_PENDING_DRAFT).eq(false));

    b1.union(UnionType::All, b2);
    b1
}

fn select_wide_table_header_projection(
    stmt: &mut SelectStatement,
    type_naming: &DocumentTypeNaming,
) {
    stmt.column(IDEN_ID)
        .column(IDEN_DISPLAY_VALUES)
        .column(IDEN_DISPLAY_TITLE)
        .expr_as(Expr::val("published"), IDEN_STATUS);

    if has_slug_column(type_naming) {
        stmt.expr_as(Expr::col(IDEN_SLUG), IDEN_SLUG);
    } else {
        stmt.expr_as(Expr::cust("NULL::text"), IDEN_SLUG);
    }

    stmt.column(IDEN_REVISION)
        .expr_as(Expr::cust("NULL::int"), IDEN_DRAFT_VERSION)
        .column(IDEN_CREATED_AT)
        .column(IDEN_CREATED_BY)
        .column(IDEN_UPDATED_AT)
        .column(IDEN_UPDATED_BY)
        .column(IDEN_PUBLISHED_AT)
        .column(IDEN_PUBLISHED_BY);
}

fn select_draft_header_projection(stmt: &mut SelectStatement) {
    stmt.column(IDEN_ID)
        .column(IDEN_DISPLAY_VALUES)
        .column(IDEN_DISPLAY_TITLE)
        .column(IDEN_STATUS)
        .expr_as(Expr::cust("NULL::text"), IDEN_SLUG)
        .expr_as(Expr::col(IDEN_LAST_PUBLISHED_REVISION), IDEN_REVISION)
        .expr_as(Expr::col(IDEN_VERSION), IDEN_DRAFT_VERSION)
        .column(IDEN_CREATED_AT)
        .column(IDEN_CREATED_BY)
        .column(IDEN_UPDATED_AT)
        .column(IDEN_UPDATED_BY)
        .expr_as(Expr::cust("NULL::timestamptz"), IDEN_PUBLISHED_AT)
        .expr_as(Expr::cust("NULL::text"), IDEN_PUBLISHED_BY);
}

fn has_slug_column(type_naming: &DocumentTypeNaming) -> bool {
    type_naming
        .field_columns
        .values()
        .any(|c| c.column_name == "slug")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use domain::common::DisplayName;
    use domain::schema::{
        AttributeId, DocumentKind, DocumentType, DocumentTypeId, DocumentTypeInfo,
        DocumentTypeOptions, FieldDefinition, FieldType, PrimitiveType, SchemaRegistry,
    };
    use sea_query::PostgresQueryBuilder;

    use crate::persistence::naming::SchemaNaming;
    use crate::persistence::query::common::compile_select;

    fn build_test_naming(with_slug: bool) -> DocumentTypeNaming {
        let mut fields = vec![FieldDefinition {
            id: AttributeId::try_new("title").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            unique: false,
            required: true,
            constraints: HashSet::new(),
        }];

        if with_slug {
            fields.push(FieldDefinition {
                id: AttributeId::try_new("slug").unwrap(),
                field_type: FieldType::Primitive(PrimitiveType::Text),
                unique: true,
                required: true,
                constraints: HashSet::new(),
            });
        }

        let type_id = DocumentTypeId::try_new("article").unwrap();
        let plural_id = DocumentTypeId::try_new("articles").unwrap();
        let doc_type = DocumentType {
            id: type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Article").unwrap(),
                singular_name: type_id.clone(),
                plural_name: plural_id,
                description: None,
                display_fields: vec![],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: fields.into_iter().collect(),
            relations: Default::default(),
        };

        let registry = SchemaRegistry::new(vec![doc_type]);
        let schema_naming = SchemaNaming::new(&registry);
        schema_naming.get(&type_id).unwrap().clone()
    }

    #[test]
    fn test_list_single_lifecycle_headers_query_with_slug() {
        let naming = build_test_naming(true);
        let pagination = Pagination { page: 1, page_size: 20 };
        let stmt = list_single_lifecycle_headers_query(&naming, pagination, None);

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("FROM \"articles\""));
        assert!(unparam.contains("'published' AS \"status\""));
        assert!(unparam.contains("\"slug\" AS \"slug\""));
        assert!(unparam.contains("NULL::int AS \"draft_version\""));
        assert!(unparam.contains("ORDER BY \"updated_at\" DESC, \"id\" ASC"));
        assert!(unparam.contains("LIMIT 20"));

        let (sql, _) = compile_select(&stmt);
        assert!(sql.contains("LIMIT"));
        assert!(sql.contains("OFFSET"));
    }

    #[test]
    fn test_list_single_lifecycle_headers_query_without_slug() {
        let naming = build_test_naming(false);
        let pagination = Pagination { page: 2, page_size: 10 };
        let stmt = list_single_lifecycle_headers_query(
            &naming,
            pagination,
            Some(DocumentStatus::Published),
        );

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("NULL::text AS \"slug\""));
        assert!(!unparam.contains("TRUE = FALSE"));

        // If filtering by non-published on single-lifecycle, impossible condition added
        let stmt_draft = list_single_lifecycle_headers_query(
            &naming,
            pagination,
            Some(DocumentStatus::Draft),
        );
        let unparam_draft = stmt_draft.to_string(PostgresQueryBuilder);
        assert!(unparam_draft.contains("TRUE = FALSE"));
    }

    #[test]
    fn test_count_single_lifecycle_headers_query() {
        let naming = build_test_naming(false);
        let stmt = count_single_lifecycle_headers_query(&naming, None);
        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("COUNT(\"id\") FROM \"articles\""));

        let stmt_unpub = count_single_lifecycle_headers_query(&naming, Some(DocumentStatus::Unpublished));
        let unparam_unpub = stmt_unpub.to_string(PostgresQueryBuilder);
        assert!(unparam_unpub.contains("TRUE = FALSE"));
    }

    #[test]
    fn test_list_draft_enabled_headers_query() {
        let naming = build_test_naming(true);
        let pagination = Pagination { page: 1, page_size: 25 };
        let stmt = list_draft_enabled_headers_query(
            &naming,
            pagination,
            Some(DocumentStatus::Modified),
        );

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("FROM \"document_drafts\""));
        assert!(unparam.contains("UNION ALL"));
        assert!(unparam.contains("FROM \"articles\""));
        assert!(unparam.contains("\"has_pending_draft\" = FALSE"));
        assert!(unparam.contains("AS \"combined\""));
        assert!(unparam.contains("\"status\" = 'modified'"));
        assert!(unparam.contains("ORDER BY \"updated_at\" DESC, \"id\" ASC"));
    }

    #[test]
    fn test_count_draft_enabled_headers_query() {
        let naming = build_test_naming(false);
        let stmt = count_draft_enabled_headers_query(&naming, Some(DocumentStatus::Draft));

        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("COUNT(\"id\") FROM (SELECT"));
        assert!(unparam.contains("UNION ALL"));
        assert!(unparam.contains("AS \"combined\" WHERE \"status\" = 'draft'"));
    }

    #[test]
    fn test_find_display_titles_by_ids_query() {
        let naming = build_test_naming(true);
        let ids = vec![Uuid::now_v7(), Uuid::now_v7()];

        // Single lifecycle
        let stmt_single = find_display_titles_by_ids_query(&naming, &ids, false);
        let unparam_single = stmt_single.to_string(PostgresQueryBuilder);
        assert!(unparam_single.contains("SELECT \"id\", \"display_title\" FROM \"articles\""));
        assert!(!unparam_single.contains("UNION ALL"));

        // Draft enabled
        let stmt_draft = find_display_titles_by_ids_query(&naming, &ids, true);
        let unparam_draft = stmt_draft.to_string(PostgresQueryBuilder);
        assert!(unparam_draft.contains("FROM \"articles\" WHERE \"has_pending_draft\" = FALSE"));
        assert!(unparam_draft.contains("UNION ALL"));
        assert!(unparam_draft.contains("FROM \"document_drafts\""));
        assert!(unparam_draft.contains("\"document_type_id\" = 'article'"));
    }
}
