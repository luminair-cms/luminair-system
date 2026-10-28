//! Sea-Query dynamic query builders for relational junction tables.
//!
//! Provides SQL statement construction for relation synchronization:
//! inserting links with ordering, batch link insertion, deleting superseded
//! associations, single-parent target lookup, and two-phase batch loading (both forward and inverse).

use sea_query::{
    DeleteStatement, Expr, ExprTrait, InsertStatement, OnConflict, Order, Query, SelectStatement,
};
use uuid::Uuid;

use domain::content::instance::DocumentInstanceId;

use crate::persistence::naming::StaticIden;
use crate::persistence::query::common::{COL_ORDER_INDEX_IDEN, COL_SOURCE_ID_IDEN, COL_TARGET_ID_IDEN};

// ---------------------------------------------------------------------------
// Public Junction Table Query Builders
// ---------------------------------------------------------------------------

/// Builds a Sea-Query insert statement for a single junction link with upsert on order index.
#[must_use]
pub fn insert_link_query(
    junction_iden: StaticIden,
    source_id: Uuid,
    target_id: Uuid,
    order_index: i32,
) -> InsertStatement {
    let mut stmt = Query::insert();
    stmt.into_table(junction_iden)
        .columns([COL_SOURCE_ID_IDEN, COL_TARGET_ID_IDEN, COL_ORDER_INDEX_IDEN])
        .values_panic([source_id.into(), target_id.into(), order_index.into()])
        .on_conflict(
            OnConflict::columns([COL_SOURCE_ID_IDEN, COL_TARGET_ID_IDEN])
                .update_column(COL_ORDER_INDEX_IDEN)
                .to_owned(),
        );
    stmt
}

/// Builds a batch insert statement for multiple links belonging to a single source instance.
///
/// Returns `None` if `target_ids` is empty.
#[must_use]
pub fn insert_links_batch_query(
    junction_iden: StaticIden,
    source_id: Uuid,
    target_ids: &[DocumentInstanceId],
) -> Option<InsertStatement> {
    if target_ids.is_empty() {
        return None;
    }

    let mut stmt = Query::insert();
    stmt.into_table(junction_iden)
        .columns([COL_SOURCE_ID_IDEN, COL_TARGET_ID_IDEN, COL_ORDER_INDEX_IDEN]);

    for (idx, target_id) in target_ids.iter().enumerate() {
        let order_index = i32::try_from(idx).unwrap_or(i32::MAX);
        stmt.values_panic([
            source_id.into(),
            target_id.into_inner().into(),
            order_index.into(),
        ]);
    }

    stmt.on_conflict(
        OnConflict::columns([COL_SOURCE_ID_IDEN, COL_TARGET_ID_IDEN])
            .update_column(COL_ORDER_INDEX_IDEN)
            .to_owned(),
    );

    Some(stmt)
}

/// Builds a delete statement removing all junction links belonging to a source instance.
#[must_use]
pub fn delete_links_by_source_query(junction_iden: StaticIden, source_id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(junction_iden)
        .and_where(Expr::col(COL_SOURCE_ID_IDEN).eq(source_id));
    stmt
}

/// Builds a select query retrieving all ordered targets for a single source instance.
#[must_use]
pub fn select_targets_by_source_query(
    junction_iden: StaticIden,
    source_id: Uuid,
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(junction_iden)
        .columns([COL_TARGET_ID_IDEN, COL_ORDER_INDEX_IDEN])
        .and_where(Expr::col(COL_SOURCE_ID_IDEN).eq(source_id))
        .order_by(COL_ORDER_INDEX_IDEN, Order::Asc);
    stmt
}

/// Builds a batch lookup query retrieving all links for a list of parent source instance IDs.
#[must_use]
pub fn select_links_by_sources_query(
    junction_iden: StaticIden,
    source_ids: &[Uuid],
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(junction_iden)
        .columns([COL_SOURCE_ID_IDEN, COL_TARGET_ID_IDEN, COL_ORDER_INDEX_IDEN])
        .and_where(Expr::col(COL_SOURCE_ID_IDEN).is_in(source_ids.to_vec()))
        .order_by(COL_SOURCE_ID_IDEN, Order::Asc)
        .order_by(COL_ORDER_INDEX_IDEN, Order::Asc);
    stmt
}

/// Builds a batch lookup query retrieving all links for a list of target instance IDs (for inverse relations).
#[must_use]
pub fn select_links_by_targets_query(
    junction_iden: StaticIden,
    target_ids: &[Uuid],
) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(junction_iden)
        .columns([COL_SOURCE_ID_IDEN, COL_TARGET_ID_IDEN, COL_ORDER_INDEX_IDEN])
        .and_where(Expr::col(COL_TARGET_ID_IDEN).is_in(target_ids.to_vec()))
        .order_by(COL_TARGET_ID_IDEN, Order::Asc)
        .order_by(COL_ORDER_INDEX_IDEN, Order::Asc);
    stmt
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sea_query::PostgresQueryBuilder;

    const JUNCTION_IDEN: StaticIden = StaticIden("articles_tags_links");

    #[test]
    fn test_insert_link_query() {
        let u1 = Uuid::nil();
        let u2 = Uuid::nil();
        let stmt = insert_link_query(JUNCTION_IDEN, u1, u2, 0);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("INSERT INTO \"articles_tags_links\" (\"source_id\", \"target_id\", \"order_index\")"));
        assert!(sql.contains("ON CONFLICT (\"source_id\", \"target_id\") DO UPDATE SET \"order_index\" = \"excluded\".\"order_index\""));
    }

    #[test]
    fn test_insert_links_batch_query_empty() {
        let u1 = Uuid::nil();
        assert!(insert_links_batch_query(JUNCTION_IDEN, u1, &[]).is_none());
    }

    #[test]
    fn test_insert_links_batch_query_multiple() {
        let u1 = Uuid::nil();
        let t1 = DocumentInstanceId::new(Uuid::nil());
        let t2 = DocumentInstanceId::new(Uuid::nil());
        let stmt = insert_links_batch_query(JUNCTION_IDEN, u1, &[t1, t2]).unwrap();
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("INSERT INTO \"articles_tags_links\""));
        assert!(sql.contains("ON CONFLICT (\"source_id\", \"target_id\") DO UPDATE SET \"order_index\" = \"excluded\".\"order_index\""));
    }

    #[test]
    fn test_delete_links_by_source_query() {
        let u1 = Uuid::nil();
        let stmt = delete_links_by_source_query(JUNCTION_IDEN, u1);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("DELETE FROM \"articles_tags_links\" WHERE \"source_id\" ="));
    }

    #[test]
    fn test_select_targets_by_source_query() {
        let u1 = Uuid::nil();
        let stmt = select_targets_by_source_query(JUNCTION_IDEN, u1);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("SELECT \"target_id\", \"order_index\" FROM \"articles_tags_links\" WHERE \"source_id\" ="));
        assert!(sql.contains("ORDER BY \"order_index\" ASC"));
    }

    #[test]
    fn test_select_links_by_sources_query() {
        let u1 = Uuid::nil();
        let stmt = select_links_by_sources_query(JUNCTION_IDEN, &[u1]);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("SELECT \"source_id\", \"target_id\", \"order_index\" FROM \"articles_tags_links\" WHERE \"source_id\" IN"));
        assert!(sql.contains("ORDER BY \"source_id\" ASC, \"order_index\" ASC"));
    }

    #[test]
    fn test_select_links_by_targets_query() {
        let u1 = Uuid::nil();
        let stmt = select_links_by_targets_query(JUNCTION_IDEN, &[u1]);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("SELECT \"source_id\", \"target_id\", \"order_index\" FROM \"articles_tags_links\" WHERE \"target_id\" IN"));
        assert!(sql.contains("ORDER BY \"target_id\" ASC, \"order_index\" ASC"));
    }
}
