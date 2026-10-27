//! Live Database Schema Introspector.
//!
//! Introspects live PostgreSQL / AWS Aurora DSQL tables, columns, constraints, and indexes
//! via `information_schema` and `pg_catalog` to construct an actual `DatabaseSchema` AST.

use std::collections::{HashMap, HashSet};

use indexmap::IndexSet;
use sqlx::PgPool;

use super::model::{
    ColumnDefinition, DatabaseSchema, ForeignKeyAction, ForeignKeyDefinition, IndexDefinition,
    SqlColumnType, TableDefinition, TableKind,
};
use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{
    TABLE_ACCESS_REQUESTS, TABLE_DOCUMENT_DRAFTS, TABLE_DOCUMENT_SNAPSHOTS,
    TABLE_DOCUMENT_TOMBSTONES, TABLE_ROLE_PERMISSIONS, TABLE_ROLES, TABLE_SHADOW_USERS,
    TABLE_USER_ROLE_ASSIGNMENTS,
};

// ---------------------------------------------------------------------------
// Constants & Types
// ---------------------------------------------------------------------------

/// Internal system and static migration tables excluded from dynamic schema management.
pub(super) const SYSTEM_TABLES: &[&str] = &[
    "_sqlx_migrations",
    TABLE_ROLES,
    TABLE_ROLE_PERMISSIONS,
    TABLE_USER_ROLE_ASSIGNMENTS,
    TABLE_ACCESS_REQUESTS,
    TABLE_SHADOW_USERS,
    TABLE_DOCUMENT_DRAFTS,
    TABLE_DOCUMENT_SNAPSHOTS,
    TABLE_DOCUMENT_TOMBSTONES,
];

type RawColumnRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
);

type RawFkRow = (String, String, String, String, String, String);

// ---------------------------------------------------------------------------
// Public Introspection Entry Point
// ---------------------------------------------------------------------------

/// Introspects the live database and returns the current actual `DatabaseSchema` AST.
///
/// # Errors
/// Returns [`PersistenceError`] if database queries fail.
pub(super) async fn introspect_database_schema(pool: &PgPool) -> Result<DatabaseSchema, PersistenceError> {
    let candidate_tables = fetch_candidate_tables(pool).await?;
    if candidate_tables.is_empty() {
        return Ok(DatabaseSchema::new());
    }

    let pk_map = fetch_primary_keys(pool).await?;
    let columns_by_table = fetch_columns(pool, &pk_map).await?;
    let indexes_by_table = fetch_indexes(pool).await?;
    let fks_by_table = fetch_foreign_keys(pool).await?;

    Ok(assemble_schema(
        candidate_tables,
        columns_by_table,
        indexes_by_table,
        fks_by_table,
    ))
}

// ---------------------------------------------------------------------------
// Private Query Workers
// ---------------------------------------------------------------------------

async fn fetch_candidate_tables(pool: &PgPool) -> Result<Vec<String>, PersistenceError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r"
        SELECT table_name::TEXT
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_type = 'BASE TABLE'
        ORDER BY table_name;
        ",
    )
    .fetch_all(pool)
    .await?;

    let candidate_tables = rows
        .into_iter()
        .map(|(t,)| t)
        .filter(|t| !SYSTEM_TABLES.contains(&t.as_str()))
        .collect();

    Ok(candidate_tables)
}

async fn fetch_primary_keys(
    pool: &PgPool,
) -> Result<HashMap<String, HashSet<String>>, PersistenceError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r"
        SELECT
            tc.table_name::TEXT,
            kcu.column_name::TEXT
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        WHERE tc.constraint_type = 'PRIMARY KEY'
          AND tc.table_schema = 'public';
        ",
    )
    .fetch_all(pool)
    .await?;

    let mut pk_map: HashMap<String, HashSet<String>> = HashMap::new();
    for (tbl, col) in rows {
        pk_map.entry(tbl).or_default().insert(col);
    }
    Ok(pk_map)
}

async fn fetch_columns(
    pool: &PgPool,
    pk_map: &HashMap<String, HashSet<String>>,
) -> Result<HashMap<String, IndexSet<ColumnDefinition>>, PersistenceError> {
    let rows: Vec<RawColumnRow> = sqlx::query_as(
        r"
        SELECT
            table_name::TEXT,
            column_name::TEXT,
            data_type::TEXT,
            is_nullable::TEXT,
            column_default::TEXT,
            character_maximum_length,
            numeric_precision,
            numeric_scale
        FROM information_schema.columns
        WHERE table_schema = 'public'
        ORDER BY table_name, ordinal_position;
        ",
    )
    .fetch_all(pool)
    .await?;

    let mut map: HashMap<String, IndexSet<ColumnDefinition>> = HashMap::new();
    for (tbl, col_name, data_type, is_nullable, col_def, char_len, num_prec, num_scale) in rows {
        if SYSTEM_TABLES.contains(&tbl.as_str()) {
            continue;
        }

        let sql_type =
            SqlColumnType::from_information_schema(&data_type, char_len, num_prec, num_scale)
                .unwrap_or(SqlColumnType::Text);

        let is_pk = pk_map.get(&tbl).is_some_and(|set| set.contains(&col_name));
        let nullable = is_nullable.eq_ignore_ascii_case("YES");

        let col = ColumnDefinition {
            name: col_name,
            data_type: sql_type,
            nullable,
            is_primary_key: is_pk,
            default_value: col_def,
        };

        map.entry(tbl).or_default().insert(col);
    }

    Ok(map)
}

async fn fetch_indexes(
    pool: &PgPool,
) -> Result<HashMap<String, IndexSet<IndexDefinition>>, PersistenceError> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r"
        SELECT
            tablename::TEXT,
            indexname::TEXT,
            indexdef::TEXT
        FROM pg_indexes
        WHERE schemaname = 'public'
        ORDER BY tablename, indexname;
        ",
    )
    .fetch_all(pool)
    .await?;

    let mut map: HashMap<String, IndexSet<IndexDefinition>> = HashMap::new();
    for (tbl, idx_name, idx_def) in rows {
        if SYSTEM_TABLES.contains(&tbl.as_str()) || idx_name.ends_with("_pkey") {
            continue;
        }

        let is_unique = idx_def.to_uppercase().contains("UNIQUE");
        let cols = extract_index_columns(&idx_def);

        let idx = IndexDefinition::new(idx_name, &tbl, cols, is_unique);
        map.entry(tbl).or_default().insert(idx);
    }

    Ok(map)
}

fn extract_index_columns(idx_def: &str) -> Vec<String> {
    if let (Some(start), Some(end)) = (idx_def.rfind('('), idx_def.rfind(')'))
        && end > start
    {
        let inside = &idx_def[start + 1..end];
        return inside
            .split(',')
            .map(|part| {
                part.trim()
                    .trim_matches('"')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();
    }
    Vec::new()
}

async fn fetch_foreign_keys(
    pool: &PgPool,
) -> Result<HashMap<String, IndexSet<ForeignKeyDefinition>>, PersistenceError> {
    let rows: Vec<RawFkRow> = sqlx::query_as(
        r"
        SELECT
            tc.table_name::TEXT,
            tc.constraint_name::TEXT,
            kcu.column_name::TEXT,
            ccu.table_name::TEXT AS foreign_table_name,
            ccu.column_name::TEXT AS foreign_column_name,
            rc.delete_rule::TEXT
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        JOIN information_schema.referential_constraints rc
          ON tc.constraint_name = rc.constraint_name
         AND tc.table_schema = rc.constraint_schema
        JOIN information_schema.constraint_column_usage ccu
          ON rc.unique_constraint_name = ccu.constraint_name
         AND rc.unique_constraint_schema = ccu.table_schema
        WHERE tc.constraint_type = 'FOREIGN KEY'
          AND tc.table_schema = 'public';
        ",
    )
    .fetch_all(pool)
    .await?;

    let mut map: HashMap<String, IndexSet<ForeignKeyDefinition>> = HashMap::new();
    for (tbl, c_name, col, ref_tbl, ref_col, del_rule) in rows {
        if SYSTEM_TABLES.contains(&tbl.as_str()) {
            continue;
        }

        let action = match del_rule.to_uppercase().as_str() {
            "CASCADE" => ForeignKeyAction::Cascade,
            "RESTRICT" => ForeignKeyAction::Restrict,
            "SET NULL" => ForeignKeyAction::SetNull,
            _ => ForeignKeyAction::NoAction,
        };

        let fk = ForeignKeyDefinition::new(
            c_name,
            vec![col],
            ref_tbl,
            vec![ref_col],
            action,
            ForeignKeyAction::NoAction,
        );

        map.entry(tbl).or_default().insert(fk);
    }

    Ok(map)
}

fn assemble_schema(
    candidate_tables: Vec<String>,
    mut columns_by_table: HashMap<String, IndexSet<ColumnDefinition>>,
    mut indexes_by_table: HashMap<String, IndexSet<IndexDefinition>>,
    mut fks_by_table: HashMap<String, IndexSet<ForeignKeyDefinition>>,
) -> DatabaseSchema {
    let mut schema = DatabaseSchema::new();

    for table_name in candidate_tables {
        let is_link = table_name.contains("_links");
        let kind = if is_link {
            TableKind::Link
        } else {
            TableKind::Entity
        };

        let mut table = TableDefinition::new(&table_name, kind);
        if let Some(cols) = columns_by_table.remove(&table_name) {
            table.columns = cols;
        }
        if let Some(idxs) = indexes_by_table.remove(&table_name) {
            table.indexes = idxs;
        }
        if let Some(fks) = fks_by_table.remove(&table_name) {
            table.foreign_keys = fks;
        }

        schema.insert_table(table);
    }

    schema
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_index_columns_single() {
        let def = "CREATE INDEX idx_articles_published_at ON public.articles USING btree (published_at DESC)";
        let cols = extract_index_columns(def);
        assert_eq!(cols, vec!["published_at"]);
    }

    #[test]
    fn test_extract_index_columns_composite() {
        let def = "CREATE INDEX idx_composite ON public.links USING btree (source_id, target_id)";
        let cols = extract_index_columns(def);
        assert_eq!(cols, vec!["source_id", "target_id"]);
    }
}
