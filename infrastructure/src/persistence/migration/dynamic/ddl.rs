//! Type-Safe DDL Compiler via `sea-query`.
//!
//! Translates AST `MigrationStep`s into idempotent PostgreSQL / Aurora DSQL DDL statements.
//! Compiles generic table definitions without hardcoded domain knowledge.

use sea_query::{
    Alias, ColumnDef, Expr, ForeignKey, ForeignKeyAction, Index, PostgresQueryBuilder, Table,
    TableCreateStatement,
};

use super::model::{
    ColumnDefinition, ForeignKeyAction as ModelFkAction, ForeignKeyDefinition, IndexDefinition,
    SqlColumnType, TableDefinition,
};
use super::step::MigrationStep;

// ---------------------------------------------------------------------------
// Public DDL Translators
// ---------------------------------------------------------------------------

/// Translates a single `MigrationStep` into a PostgreSQL DDL string using `sea-query`.
#[must_use]
pub fn step_to_sql(step: &MigrationStep) -> String {
    match step {
        MigrationStep::CreateTable(table) => build_create_table_sql(table),
        MigrationStep::DropTable { name, .. } => build_drop_table_sql(name),
        MigrationStep::AddColumn { table, column } => build_add_column_sql(table, column),
        MigrationStep::DropColumn { table, column } => build_drop_column_sql(table, column),
        MigrationStep::CreateIndex(index) => build_create_index_sql(index),
        MigrationStep::DropIndex { name, .. } => build_drop_index_sql(name),
    }
}

/// Generates a `CREATE TABLE IF NOT EXISTS` statement from a `TableDefinition`.
///
/// Automatically determines whether to emit inline `PRIMARY KEY` (for single PK columns)
/// or a table-level composite `PRIMARY KEY (...)` constraint (for multiple PK columns)
/// without requiring table-kind branching.
#[must_use]
pub fn build_create_table_sql(table: &TableDefinition) -> String {
    let mut stmt = Table::create();
    stmt.table(Alias::new(&table.name)).if_not_exists();

    let pk_cols = table.primary_key_columns();
    let is_composite_pk = pk_cols.len() > 1;

    for col in &table.columns {
        let include_inline_pk = !is_composite_pk && col.is_primary_key;
        let mut col_def = build_column_def_internal(col, include_inline_pk);
        stmt.col(&mut col_def);
    }

    if is_composite_pk {
        let mut pk_idx = Index::create();
        for col in pk_cols {
            pk_idx.col(Alias::new(col));
        }
        stmt.primary_key(&mut pk_idx);
    }

    for fk in &table.foreign_keys {
        append_foreign_key(&mut stmt, &table.name, fk);
    }

    stmt.to_string(PostgresQueryBuilder)
}

/// Generates a `DROP TABLE IF EXISTS` statement.
#[must_use]
pub fn build_drop_table_sql(table_name: &str) -> String {
    Table::drop()
        .table(Alias::new(table_name))
        .if_exists()
        .to_string(PostgresQueryBuilder)
}

/// Generates an `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` statement.
#[must_use]
pub fn build_add_column_sql(table_name: &str, column: &ColumnDefinition) -> String {
    let mut col_def = build_column_def(column);
    Table::alter()
        .table(Alias::new(table_name))
        .add_column_if_not_exists(&mut col_def)
        .to_string(PostgresQueryBuilder)
}

/// Generates an `ALTER TABLE ... DROP COLUMN IF EXISTS` statement.
#[must_use]
pub fn build_drop_column_sql(table_name: &str, column_name: &str) -> String {
    Table::alter()
        .table(Alias::new(table_name))
        .drop_column_if_exists(Alias::new(column_name))
        .to_string(PostgresQueryBuilder)
}

/// Generates a `CREATE [UNIQUE] INDEX IF NOT EXISTS` statement from an `IndexDefinition`.
#[must_use]
pub fn build_create_index_sql(index: &IndexDefinition) -> String {
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

/// Generates a `DROP INDEX IF EXISTS` statement.
#[must_use]
pub fn build_drop_index_sql(index_name: &str) -> String {
    Index::drop()
        .name(index_name)
        .if_exists()
        .to_string(PostgresQueryBuilder)
}

/// Maps a `ColumnDefinition` to a typed `sea_query::ColumnDef`.
#[must_use]
pub fn build_column_def(col: &ColumnDefinition) -> ColumnDef {
    build_column_def_internal(col, col.is_primary_key)
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn build_column_def_internal(col: &ColumnDefinition, include_pk: bool) -> ColumnDef {
    let mut def = ColumnDef::new(Alias::new(&col.name));

    apply_sql_data_type(&mut def, &col.data_type);

    if !col.nullable {
        def.not_null();
    }

    if include_pk && col.is_primary_key {
        def.primary_key();
    }

    apply_default_value(&mut def, col.default_value.as_deref());

    def
}

fn apply_sql_data_type(def: &mut ColumnDef, data_type: &SqlColumnType) {
    match data_type {
        SqlColumnType::Uuid => {
            def.uuid();
        }
        SqlColumnType::Varchar(Some(len)) => {
            def.string_len(u32::try_from(*len).unwrap_or(u32::MAX));
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
            def.decimal_len(u32::from(*p), u32::from(*s));
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
}

fn apply_default_value(def: &mut ColumnDef, default_val: Option<&str>) {
    if let Some(val) = default_val {
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
}

fn append_foreign_key(
    stmt: &mut TableCreateStatement,
    table_name: &str,
    fk: &ForeignKeyDefinition,
) {
    let mut fk_stmt = ForeignKey::create();
    fk_stmt.name(&fk.name);
    for col in &fk.columns {
        fk_stmt.from(Alias::new(table_name), Alias::new(col));
    }
    for ref_col in &fk.referenced_columns {
        fk_stmt.to(Alias::new(&fk.referenced_table), Alias::new(ref_col));
    }
    fk_stmt.on_delete(map_fk_action(fk.on_delete));
    fk_stmt.on_update(map_fk_action(fk.on_update));
    stmt.foreign_key(&mut fk_stmt);
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
        assert!(sql.contains(
            r#"FOREIGN KEY ("source_id") REFERENCES "articles" ("id") ON DELETE CASCADE"#
        ));
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
            "idx_articles_status",
            "articles",
            vec!["status".into()],
            false,
        );
        let step = MigrationStep::CreateIndex(idx);
        let sql = step_to_sql(&step);

        assert_eq!(
            sql,
            r#"CREATE INDEX IF NOT EXISTS "idx_articles_status" ON "articles" ("status")"#
        );
    }

    #[test]
    fn test_step_to_sql_drop_table() {
        let step = MigrationStep::DropTable {
            name: "old_articles".into(),
            kind: TableKind::Entity,
        };
        let sql = step_to_sql(&step);

        assert_eq!(sql, r#"DROP TABLE IF EXISTS "old_articles""#);
    }

    #[test]
    fn test_step_to_sql_drop_column() {
        let step = MigrationStep::DropColumn {
            table: "articles".into(),
            column: "legacy_field".into(),
        };
        let sql = step_to_sql(&step);

        assert_eq!(
            sql,
            r#"ALTER TABLE "articles" DROP COLUMN IF EXISTS "legacy_field""#
        );
    }

    #[test]
    fn test_step_to_sql_drop_index() {
        let step = MigrationStep::DropIndex {
            name: "idx_old".into(),
            table: "articles".into(),
        };
        let sql = step_to_sql(&step);

        assert_eq!(sql, r#"DROP INDEX IF EXISTS "idx_old""#);
    }
}
