//! Sea-Query dynamic query builders for relational junction tables.
//!
//! Provides SQL generation for relation synchronization: inserting links with ordering,
//! deleting superseded associations, single-parent target lookup, and two-phase batch loading.

use sea_query::{
    Alias, Expr, ExprTrait, InsertStatement, OnConflict, Order, Query, SelectStatement,
};
use uuid::Uuid;

use crate::persistence::naming::{COL_ORDER_INDEX, COL_SOURCE_ID, COL_TARGET_ID};

// ---------------------------------------------------------------------------
// Public Junction Table Query Builders
// ---------------------------------------------------------------------------

/// Builds a Sea-Query insert statement for a single junction link with upsert on order index.
#[must_use]
pub fn insert_link_query(
    junction_table: &str,
    source_id: Uuid,
    target_id: Uuid,
    order_index: i32,
) -> InsertStatement {
    let mut stmt = Query::insert();
    stmt.into_table(Alias::new(junction_table))
        .columns([
            Alias::new(COL_SOURCE_ID),
            Alias::new(COL_TARGET_ID),
            Alias::new(COL_ORDER_INDEX),
        ])
        .values_panic([source_id.into(), target_id.into(), order_index.into()])
        .on_conflict(
            OnConflict::columns([Alias::new(COL_SOURCE_ID), Alias::new(COL_TARGET_ID)])
                .update_column(Alias::new(COL_ORDER_INDEX))
                .to_owned(),
        );
    stmt
}

/// Builds a parameterized SQL string for inserting a junction link with order upsert.
#[must_use]
pub fn insert_link_sql(junction_table: &str) -> String {
    format!(
        "INSERT INTO {junction_table} ({COL_SOURCE_ID}, {COL_TARGET_ID}, {COL_ORDER_INDEX})\nVALUES ($1, $2, $3)\nON CONFLICT ({COL_SOURCE_ID}, {COL_TARGET_ID}) DO UPDATE SET\n    {COL_ORDER_INDEX} = EXCLUDED.{COL_ORDER_INDEX};"
    )
}

/// Builds a parameterized SQL string for deleting all junction links belonging to a source instance.
#[must_use]
pub fn delete_links_by_source_sql(junction_table: &str) -> String {
    format!("DELETE FROM {junction_table} WHERE {COL_SOURCE_ID} = $1;")
}

/// Builds a parameterized SQL string for deleting a specific link between source and target.
#[must_use]
pub fn delete_link_sql(junction_table: &str) -> String {
    format!("DELETE FROM {junction_table} WHERE {COL_SOURCE_ID} = $1 AND {COL_TARGET_ID} = $2;")
}

/// Builds a parameterized SQL string for selecting all ordered targets for a single source.
#[must_use]
pub fn select_targets_by_source_sql(junction_table: &str) -> String {
    format!(
        "SELECT {COL_TARGET_ID}, {COL_ORDER_INDEX} FROM {junction_table} WHERE {COL_SOURCE_ID} = $1 ORDER BY {COL_ORDER_INDEX} ASC;"
    )
}

/// Builds a batch lookup query retrieving all links for a list of parent source instance IDs.
#[must_use]
pub fn select_links_by_sources_query(junction_table: &str, source_ids: &[Uuid]) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(Alias::new(junction_table))
        .columns([
            Alias::new(COL_SOURCE_ID),
            Alias::new(COL_TARGET_ID),
            Alias::new(COL_ORDER_INDEX),
        ])
        .and_where(Expr::col(Alias::new(COL_SOURCE_ID)).is_in(source_ids.to_vec()))
        .order_by(Alias::new(COL_SOURCE_ID), Order::Asc)
        .order_by(Alias::new(COL_ORDER_INDEX), Order::Asc);
    stmt
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::PostgresQueryBuilder;

    #[test]
    fn test_insert_link_query() {
        let u1 = Uuid::nil();
        let u2 = Uuid::nil();
        let stmt = insert_link_query("articles_tags_links", u1, u2, 0);
        let sql = stmt.to_string(PostgresQueryBuilder);

        assert!(sql.contains("INSERT INTO \"articles_tags_links\""));
        assert!(sql.contains("\"source_id\", \"target_id\", \"order_index\""));
        assert!(sql.contains("ON CONFLICT (\"source_id\", \"target_id\") DO UPDATE SET"));
        assert!(sql.contains("\"order_index\" = \"excluded\".\"order_index\""));
    }

    #[test]
    fn test_insert_link_sql() {
        let sql = insert_link_sql("articles_tags_links");
        assert!(
            sql.starts_with("INSERT INTO articles_tags_links (source_id, target_id, order_index)")
        );
        assert!(sql.contains("VALUES ($1, $2, $3)"));
        assert!(sql.contains("ON CONFLICT (source_id, target_id) DO UPDATE SET"));
    }

    #[test]
    fn test_delete_links_by_source_sql() {
        let sql = delete_links_by_source_sql("articles_tags_links");
        assert_eq!(sql, "DELETE FROM articles_tags_links WHERE source_id = $1;");
    }

    #[test]
    fn test_delete_link_sql() {
        let sql = delete_link_sql("articles_tags_links");
        assert_eq!(
            sql,
            "DELETE FROM articles_tags_links WHERE source_id = $1 AND target_id = $2;"
        );
    }

    #[test]
    fn test_select_targets_by_source_sql() {
        let sql = select_targets_by_source_sql("articles_tags_links");
        assert_eq!(
            sql,
            "SELECT target_id, order_index FROM articles_tags_links WHERE source_id = $1 ORDER BY order_index ASC;"
        );
    }

    #[test]
    fn test_select_links_by_sources_query() {
        let ids = vec![Uuid::nil()];
        let stmt = select_links_by_sources_query("articles_tags_links", &ids);
        let sql = stmt.to_string(PostgresQueryBuilder);

        assert!(sql.contains(
            "SELECT \"source_id\", \"target_id\", \"order_index\" FROM \"articles_tags_links\""
        ));
        assert!(sql.contains("\"source_id\" IN"));
        assert!(sql.contains("ORDER BY \"source_id\" ASC, \"order_index\" ASC"));
    }
}
