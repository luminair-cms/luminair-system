//! Type-Safe DDL Generation via `sea-query` and Database Execution.
//!
//! Converts `MigrationStep`s into idempotent PostgreSQL / Aurora DSQL DDL statements
//! and executes each statement independently outside transaction blocks.

use sea_query::{
    Alias, ColumnDef, Expr, ForeignKey, ForeignKeyAction, Index, PostgresQueryBuilder, Table,
};
use sqlx::PgPool;

use super::diff::MigrationStep;
use super::model::{
    ColumnDefinition, ForeignKeyAction as ModelFkAction, IndexDefinition, SqlColumnType,
    TableDefinition,
};
use super::planner::MigrationPlan;
use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{COL_SOURCE_ID, COL_TARGET_ID};

// ---------------------------------------------------------------------------
// Public Execution Types
// ---------------------------------------------------------------------------

/// Result summary of executing a migration plan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionSummary {
    pub executed_statements: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public DDL Translators & Executor
// ---------------------------------------------------------------------------

/// Translates a single `MigrationStep` into a PostgreSQL DDL string using `sea-query`.
pub fn step_to_sql(step: &MigrationStep) -> String {
    match step {
        MigrationStep::CreateTable(table) => build_create_table_sql(table),
        MigrationStep::DropTable { name, .. } => Table::drop()
            .table(Alias::new(name))
            .if_exists()
            .to_string(PostgresQueryBuilder),
        MigrationStep::AddColumn { table, column } => {
            let mut col_def = build_column_def(column);
            Table::alter()
                .table(Alias::new(table))
                .add_column_if_not_exists(&mut col_def)
                .to_string(PostgresQueryBuilder)
        }
        MigrationStep::DropColumn { table, column } => Table::alter()
            .table(Alias::new(table))
            .drop_column_if_exists(Alias::new(column))
            .to_string(PostgresQueryBuilder),
        MigrationStep::CreateIndex(index) => build_create_index_sql(index),
        MigrationStep::DropIndex { name, .. } => Index::drop()
            .name(name)
            .if_exists()
            .to_string(PostgresQueryBuilder),
    }
}

/// Executes a `MigrationPlan` statement-by-statement outside transaction blocks.
pub async fn execute_migration_plan(
    pool: &PgPool,
    plan: &MigrationPlan,
) -> Result<ExecutionSummary, PersistenceError> {
    let mut executed = Vec::new();

    for step in &plan.steps {
        let sql = step_to_sql(step);
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(pool)
            .await?;
        executed.push(sql);
    }

    Ok(ExecutionSummary {
        executed_statements: executed,
    })
}

/// Maps a `ColumnDefinition` to a typed `sea_query::ColumnDef`.
pub fn build_column_def(col: &ColumnDefinition) -> ColumnDef {
    build_column_def_internal(col, col.is_primary_key)
}

// ---------------------------------------------------------------------------
// Private DDL Builders
// ---------------------------------------------------------------------------

fn build_create_table_sql(table: &TableDefinition) -> String {
    let mut stmt = Table::create();
    stmt.table(Alias::new(&table.name)).if_not_exists();

    if table.is_junction() {
        for col in &table.columns {
            let mut col_def = build_column_def_internal(col, false);
            stmt.col(&mut col_def);
        }
        stmt.primary_key(
            Index::create()
                .col(Alias::new(COL_SOURCE_ID))
                .col(Alias::new(COL_TARGET_ID)),
        );
    } else {
        for col in &table.columns {
            let mut col_def = build_column_def(col);
            stmt.col(&mut col_def);
        }
    }

    for fk in &table.foreign_keys {
        let mut fk_stmt = ForeignKey::create();
        fk_stmt.name(&fk.name);
        for col in &fk.columns {
            fk_stmt.from(Alias::new(&table.name), Alias::new(col));
        }
        for ref_col in &fk.referenced_columns {
            fk_stmt.to(Alias::new(&fk.referenced_table), Alias::new(ref_col));
        }
        fk_stmt.on_delete(map_fk_action(fk.on_delete));
        fk_stmt.on_update(map_fk_action(fk.on_update));
        stmt.foreign_key(&mut fk_stmt);
    }

    stmt.to_string(PostgresQueryBuilder)
}

fn build_create_index_sql(index: &IndexDefinition) -> String {
    let mut stmt = Index::create();
    stmt.if_not_exists()
        .name(&index.name)
        .table(Alias::new(&index.table_name));

    if index.unique {
        stmt.unique();
    }

    for col in &index.columns {
        stmt.col(Alias::new(col));
    }

    stmt.to_string(PostgresQueryBuilder)
}

fn build_column_def_internal(col: &ColumnDefinition, include_pk: bool) -> ColumnDef {
    let mut def = ColumnDef::new(Alias::new(&col.name));

    match col.data_type {
        SqlColumnType::Uuid => {
            def.uuid();
        }
        SqlColumnType::Varchar(Some(len)) => {
            def.string_len(len as u32);
        }
        SqlColumnType::Varchar(None) => {
            def.string();
        }
        SqlColumnType::Text => {
            def.text();
        }
        SqlColumnType::SmallInt => {
            def.small_integer();
        }
        SqlColumnType::Integer => {
            def.integer();
        }
        SqlColumnType::BigInt => {
            def.big_integer();
        }
        SqlColumnType::Decimal(p, s) => {
            def.decimal_len(p as u32, s as u32);
        }
        SqlColumnType::Boolean => {
            def.boolean();
        }
        SqlColumnType::Date => {
            def.date();
        }
        SqlColumnType::Timestamptz => {
            def.timestamp_with_time_zone();
        }
        SqlColumnType::Jsonb => {
            def.json_binary();
        }
    }

    if !col.nullable {
        def.not_null();
    }

    if include_pk && col.is_primary_key {
        def.primary_key();
    }

    if let Some(ref val) = col.default_value {
        if val.eq_ignore_ascii_case("CURRENT_TIMESTAMP") {
            def.default(Expr::current_timestamp());
        } else if val.eq_ignore_ascii_case("TRUE") {
            def.default(true);
        } else if val.eq_ignore_ascii_case("FALSE") {
            def.default(false);
        } else if let Ok(n) = val.parse::<i64>() {
            def.default(n);
        }
    }

    def
}

fn map_fk_action(action: ModelFkAction) -> ForeignKeyAction {
    match action {
        ModelFkAction::Cascade => ForeignKeyAction::Cascade,
        ModelFkAction::Restrict => ForeignKeyAction::Restrict,
        ModelFkAction::SetNull => ForeignKeyAction::SetNull,
        ModelFkAction::NoAction => ForeignKeyAction::NoAction,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::migration::dynamic::model::{
        ColumnDefinition, ForeignKeyDefinition, IndexDefinition, SqlColumnType, TableDefinition,
        TableKind,
    };

    #[test]
    fn test_step_to_sql_create_table() {
        let mut table = TableDefinition::new("articles", TableKind::Entity, false);
        table.columns.insert(ColumnDefinition::new(
            "id",
            SqlColumnType::Uuid,
            false,
            true,
        ));
        table.columns.insert(ColumnDefinition::new(
            "title",
            SqlColumnType::Text,
            false,
            false,
        ));

        let step = MigrationStep::CreateTable(table);
        let sql = step_to_sql(&step);

        assert!(sql.starts_with(r#"CREATE TABLE IF NOT EXISTS "articles""#));
        assert!(sql.contains(r#""id" uuid NOT NULL PRIMARY KEY"#));
        assert!(sql.contains(r#""title" text NOT NULL"#));
    }

    #[test]
    fn test_step_to_sql_link_table_with_composite_pk_and_fk() {
        let mut table = TableDefinition::new("articles_tags_links", TableKind::Link, false);
        table.columns.insert(ColumnDefinition::new(
            "source_id",
            SqlColumnType::Uuid,
            false,
            true,
        ));
        table.columns.insert(ColumnDefinition::new(
            "target_id",
            SqlColumnType::Uuid,
            false,
            true,
        ));
        table.columns.insert(ColumnDefinition::new(
            "order_index",
            SqlColumnType::Integer,
            false,
            false,
        ));

        table.foreign_keys.insert(ForeignKeyDefinition::new(
            "fk_articles_tags_links_source",
            vec!["source_id".into()],
            "articles",
            vec!["id".into()],
            ModelFkAction::Cascade,
            ModelFkAction::NoAction,
        ));

        let step = MigrationStep::CreateTable(table);
        let sql = step_to_sql(&step);

        assert!(sql.starts_with(r#"CREATE TABLE IF NOT EXISTS "articles_tags_links""#));
        assert!(sql.contains(r#"PRIMARY KEY ("source_id", "target_id")"#));
        assert!(sql.contains(r#"FOREIGN KEY ("source_id") REFERENCES "articles" ("id") ON DELETE CASCADE"#));
    }

    #[test]
    fn test_step_to_sql_add_column() {
        let col = ColumnDefinition::new("summary", SqlColumnType::Text, true, false);
        let step = MigrationStep::AddColumn {
            table: "articles".into(),
            column: col,
        };
        let sql = step_to_sql(&step);
        assert_eq!(
            sql,
            r#"ALTER TABLE "articles" ADD COLUMN IF NOT EXISTS "summary" text"#
        );
    }

    #[test]
    fn test_step_to_sql_create_index() {
        let idx = IndexDefinition::new(
            "idx_articles_published_at",
            "articles",
            vec!["published_at".into()],
            false,
        );
        let step = MigrationStep::CreateIndex(idx);
        let sql = step_to_sql(&step);
        assert_eq!(
            sql,
            r#"CREATE INDEX IF NOT EXISTS "idx_articles_published_at" ON "articles" ("published_at")"#
        );
    }
}
