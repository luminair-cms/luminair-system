//! Sea-Query dynamic query builders for document wide tables.
//!
//! Provides SQL statement construction for published wide table operations:
//! single-instance reads, visitor pagination queries, filters, counts, and upserts.

use domain::content::ports::{FieldFilter, Pagination};
use domain::content::values::{DomainValue, PrimitiveValue};
use sea_query::{Alias, Asterisk, Expr, ExprTrait, Func, Order, Query, SelectStatement};
use uuid::Uuid;

use crate::persistence::naming::{
    COL_CREATED_AT, COL_CREATED_BY, COL_DISPLAY_TITLE, COL_DISPLAY_VALUES, COL_HAS_PENDING_DRAFT,
    COL_ID, COL_PUBLISHED_AT, COL_PUBLISHED_BY, COL_REVISION, COL_UPDATED_AT, COL_UPDATED_BY,
    column_name,
};

// ---------------------------------------------------------------------------
// Value Translation
// ---------------------------------------------------------------------------

/// Converts a domain value to a Sea-Query SQL parameter value.
#[must_use]
pub fn domain_value_to_sea_value(val: &DomainValue) -> sea_query::Value {
    match val {
        DomainValue::Primitive(p) => match p {
            PrimitiveValue::Text(s) | PrimitiveValue::Uid(s) => s.clone().into(),
            PrimitiveValue::Uuid(u) => (*u).into(),
            PrimitiveValue::Integer(i) => (*i).into(),
            PrimitiveValue::Decimal(d) => (*d).into(),
            PrimitiveValue::Date(d) => (*d).into(),
            PrimitiveValue::DateTime(dt) => (*dt).into(),
            PrimitiveValue::Boolean(b) => (*b).into(),
        },
        DomainValue::Email(e) => e.as_ref().to_string().into(),
        DomainValue::Url(u) => u.as_ref().to_string().into(),
        DomainValue::Json(j) => {
            let map: serde_json::Map<String, serde_json::Value> = j
                .iter()
                .map(|(k, v)| (k.clone(), primitive_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map).into()
        }
    }
}

fn primitive_value_to_json(p: &PrimitiveValue) -> serde_json::Value {
    match p {
        PrimitiveValue::Text(s) | PrimitiveValue::Uid(s) => serde_json::Value::String(s.clone()),
        PrimitiveValue::Uuid(u) => serde_json::Value::String(u.to_string()),
        PrimitiveValue::Integer(i) => serde_json::Value::Number((*i).into()),
        PrimitiveValue::Decimal(d) => serde_json::Value::String(d.to_string()),
        PrimitiveValue::Date(d) => serde_json::Value::String(d.to_string()),
        PrimitiveValue::DateTime(dt) => serde_json::Value::String(dt.to_rfc3339()),
        PrimitiveValue::Boolean(b) => serde_json::Value::Bool(*b),
    }
}

// ---------------------------------------------------------------------------
// Public Wide Table Query Builders
// ---------------------------------------------------------------------------

/// Builds a `SELECT * FROM {table_name} WHERE id = $1` query statement.
#[must_use]
pub fn select_by_id_query(table_name: &str, id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(Alias::new(table_name))
        .column(Asterisk)
        .and_where(Expr::col(Alias::new(COL_ID)).eq(id));
    stmt
}

/// Builds a parameterized SQL string for `SELECT * FROM {table_name} WHERE id = $1`.
#[must_use]
pub fn select_by_id_sql(table_name: &str) -> String {
    format!("SELECT * FROM {table_name} WHERE {COL_ID} = $1;")
}

/// Builds a paginated `find_published` query with field equality filters.
#[must_use]
pub fn find_published_query(
    table_name: &str,
    filters: &[FieldFilter],
    pagination: Pagination,
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(Alias::new(table_name))
        .column(Asterisk)
        .and_where(Expr::col(Alias::new(COL_HAS_PENDING_DRAFT)).eq(false));

    apply_field_filters(&mut stmt, filters);

    let page_offset = u64::from(pagination.page.saturating_sub(1));
    let page_size = u64::from(pagination.page_size);
    let offset = page_offset * page_size;
    stmt.order_by(Alias::new(COL_PUBLISHED_AT), Order::Desc)
        .limit(page_size)
        .offset(offset);

    stmt
}

/// Builds a `COUNT(*)` query for published rows matching the given filters.
#[must_use]
pub fn count_published_query(table_name: &str, filters: &[FieldFilter]) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(Alias::new(table_name))
        .expr(Func::count(Expr::col(Alias::new(COL_ID))))
        .and_where(Expr::col(Alias::new(COL_HAS_PENDING_DRAFT)).eq(false));

    apply_field_filters(&mut stmt, filters);
    stmt
}

/// Builds a batch `SELECT id, display_title FROM {table_name} WHERE id IN (...)` query.
#[must_use]
pub fn find_display_titles_by_ids_query(table_name: &str, ids: &[Uuid]) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(Alias::new(table_name))
        .columns([Alias::new(COL_ID), Alias::new(COL_DISPLAY_TITLE)])
        .and_where(Expr::col(Alias::new(COL_ID)).is_in(ids.to_vec()));
    stmt
}

/// Builds the SQL string for updating the `has_pending_draft` flag on a wide table row.
#[must_use]
pub fn update_has_pending_draft_sql(table_name: &str) -> String {
    format!("UPDATE {table_name} SET {COL_HAS_PENDING_DRAFT} = $1 WHERE {COL_ID} = $2;")
}

/// Builds the SQL string for deleting a row from a wide table by primary key.
#[must_use]
pub fn delete_by_id_sql(table_name: &str) -> String {
    format!("DELETE FROM {table_name} WHERE {COL_ID} = $1;")
}

/// Builds the SQL string for upserting a published document into its wide table.
#[must_use]
pub fn build_upsert_wide_table_sql(table_name: &str, custom_columns: &[String]) -> String {
    let mut columns = vec![
        COL_ID.to_string(),
        COL_HAS_PENDING_DRAFT.to_string(),
        COL_DISPLAY_VALUES.to_string(),
        COL_DISPLAY_TITLE.to_string(),
    ];
    for col in custom_columns {
        columns.push(col.clone());
    }
    columns.extend([
        COL_REVISION.to_string(),
        COL_PUBLISHED_AT.to_string(),
        COL_PUBLISHED_BY.to_string(),
        COL_CREATED_AT.to_string(),
        COL_CREATED_BY.to_string(),
        COL_UPDATED_AT.to_string(),
        COL_UPDATED_BY.to_string(),
    ]);

    let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${i}")).collect();

    let mut update_clauses = vec![
        format!("{COL_HAS_PENDING_DRAFT} = EXCLUDED.{COL_HAS_PENDING_DRAFT}"),
        format!("{COL_DISPLAY_VALUES} = EXCLUDED.{COL_DISPLAY_VALUES}"),
        format!("{COL_DISPLAY_TITLE} = EXCLUDED.{COL_DISPLAY_TITLE}"),
    ];
    for col in custom_columns {
        update_clauses.push(format!("{col} = EXCLUDED.{col}"));
    }
    update_clauses.extend([
        format!("{COL_REVISION} = EXCLUDED.{COL_REVISION}"),
        format!("{COL_PUBLISHED_AT} = EXCLUDED.{COL_PUBLISHED_AT}"),
        format!("{COL_PUBLISHED_BY} = EXCLUDED.{COL_PUBLISHED_BY}"),
        format!("{COL_UPDATED_AT} = EXCLUDED.{COL_UPDATED_AT}"),
        format!("{COL_UPDATED_BY} = EXCLUDED.{COL_UPDATED_BY}"),
    ]);

    format!(
        "INSERT INTO {table_name} ({cols})\nVALUES ({vals})\nON CONFLICT ({COL_ID}) DO UPDATE SET\n    {updates};",
        cols = columns.join(", "),
        vals = placeholders.join(", "),
        updates = update_clauses.join(",\n    ")
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn apply_field_filters(stmt: &mut SelectStatement, filters: &[FieldFilter]) {
    for filter in filters {
        let col = column_name(&filter.attribute_id);
        let sea_val = domain_value_to_sea_value(&filter.value);
        stmt.and_where(Expr::col(Alias::new(col)).eq(sea_val));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::schema::AttributeId;
    use sea_query::PostgresQueryBuilder;

    #[test]
    fn test_select_by_id_query() {
        let id = Uuid::nil();
        let stmt = select_by_id_query("articles", id);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("SELECT * FROM \"articles\""));
        assert!(sql.contains("\"id\" ="));
    }

    #[test]
    fn test_find_published_query() {
        let filter = FieldFilter {
            attribute_id: AttributeId::try_new("slug").unwrap(),
            value: DomainValue::Primitive(PrimitiveValue::Uid("rust-rocks".to_string())),
        };
        let pagination = Pagination {
            page: 2,
            page_size: 10,
        };
        let stmt = find_published_query("articles", &[filter], pagination);
        let sql = stmt.to_string(PostgresQueryBuilder);

        assert!(sql.contains("SELECT * FROM \"articles\""));
        assert!(sql.contains("\"has_pending_draft\" = FALSE"));
        assert!(sql.contains("\"slug\" = 'rust-rocks'"));
        assert!(sql.contains("ORDER BY \"published_at\" DESC"));
        assert!(sql.contains("LIMIT 10"));
        assert!(sql.contains("OFFSET 10"));
    }

    #[test]
    fn test_count_published_query() {
        let stmt = count_published_query("articles", &[]);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("COUNT(\"id\")"));
        assert!(sql.contains("\"has_pending_draft\" = FALSE"));
    }

    #[test]
    fn test_build_upsert_wide_table_sql() {
        let sql = build_upsert_wide_table_sql("articles", &["slug".into(), "title".into()]);
        assert!(sql.starts_with("INSERT INTO articles (id, has_pending_draft, display_values, display_title, slug, title, revision, published_at, published_by, created_at, created_by, updated_at, updated_by)"));
        assert!(sql.contains("ON CONFLICT (id) DO UPDATE SET"));
        assert!(sql.contains("slug = EXCLUDED.slug"));
        assert!(sql.contains("title = EXCLUDED.title"));
    }

    #[test]
    fn test_find_display_titles_by_ids_query() {
        let u1 = Uuid::nil();
        let stmt = find_display_titles_by_ids_query("articles", &[u1]);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("\"id\", \"display_title\""));
        assert!(sql.contains("\"id\" IN"));
    }
}
