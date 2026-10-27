//! Discrete DDL migration step AST and safety policy.
//!
//! Represents atomic schema operations (create/drop table, add/drop column, create/drop index)
//! decoupled from SQL dialect generators and database execution engines.

use serde::{Deserialize, Serialize};

use super::model::{ColumnDefinition, IndexDefinition, TableDefinition, TableKind};

// ---------------------------------------------------------------------------
// Safety Policy & Migration Steps
// ---------------------------------------------------------------------------

/// Safety policy governing schema migration step generation and execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SafetyPolicy {
    /// Non-destructive changes only: provisions missing tables, columns, and indexes.
    #[default]
    #[serde(alias = "AdditiveOnly", alias = "additive", alias = "additive_only")]
    AdditiveOnly,
    /// Allows dropping unmanaged tables, columns, or indexes (for test tear-down or local resets).
    #[serde(alias = "AllowDestructive", alias = "destructive", alias = "allow_destructive")]
    AllowDestructive,
}

/// A discrete DDL migration operation AST.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum MigrationStep {
    /// Create a new relational table with its columns, primary keys, and foreign keys.
    CreateTable(TableDefinition),
    /// Drop an existing table.
    DropTable {
        name: String,
        kind: TableKind,
    },
    /// Add a column to an existing table.
    AddColumn {
        table: String,
        column: ColumnDefinition,
    },
    /// Drop a column from an existing table.
    DropColumn {
        table: String,
        column: String,
    },
    /// Create a secondary index on a table.
    CreateIndex(IndexDefinition),
    /// Drop an index.
    DropIndex {
        name: String,
    },
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl MigrationStep {
    /// Formats a human-readable summary of this migration step.
    #[must_use]
    pub(super) fn describe(&self) -> String {
        match self {
            Self::CreateTable(table) => format!(
                "CREATE TABLE \"{}\" ({} columns, {} foreign keys)",
                table.name,
                table.columns.len(),
                table.foreign_keys.len()
            ),
            Self::DropTable { name, .. } => format!("DROP TABLE \"{name}\""),
            Self::AddColumn { table, column } => format!(
                "ADD COLUMN \"{}\".\"{}\" ({})",
                table,
                column.name,
                column.data_type.to_sql_string()
            ),
            Self::DropColumn { table, column } => {
                format!("DROP COLUMN \"{table}\".\"{column}\"")
            }
            Self::CreateIndex(idx) => {
                let kind = if idx.unique { "UNIQUE INDEX" } else { "INDEX" };
                format!(
                    "CREATE {} \"{}\" ON \"{}\" ({})",
                    kind,
                    idx.name,
                    idx.table_name,
                    idx.columns.join(", ")
                )
            }
            Self::DropIndex { name } => format!("DROP INDEX \"{name}\""),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::model::{
        ColumnDefinition, IndexDefinition, SqlColumnType, TableDefinition, TableKind,
    };

    #[test]
    fn test_describe_all_variants() {
        let mut table = TableDefinition::new("articles", TableKind::Entity);
        table
            .columns
            .insert(ColumnDefinition::new("id", SqlColumnType::Uuid, false, true));
        let step_create_table = MigrationStep::CreateTable(table);
        assert_eq!(
            step_create_table.describe(),
            "CREATE TABLE \"articles\" (1 columns, 0 foreign keys)"
        );

        let step_drop_table = MigrationStep::DropTable {
            name: "articles".to_string(),
            kind: TableKind::Entity,
        };
        assert_eq!(step_drop_table.describe(), "DROP TABLE \"articles\"");

        let step_add_col = MigrationStep::AddColumn {
            table: "articles".to_string(),
            column: ColumnDefinition::new("title", SqlColumnType::Text, false, false),
        };
        assert_eq!(
            step_add_col.describe(),
            "ADD COLUMN \"articles\".\"title\" (TEXT)"
        );

        let step_drop_col = MigrationStep::DropColumn {
            table: "articles".to_string(),
            column: "obsolete".to_string(),
        };
        assert_eq!(
            step_drop_col.describe(),
            "DROP COLUMN \"articles\".\"obsolete\""
        );

        let step_create_idx = MigrationStep::CreateIndex(IndexDefinition::new(
            "idx_articles_status",
            "articles",
            vec!["status".to_string()],
            false,
        ));
        assert_eq!(
            step_create_idx.describe(),
            "CREATE INDEX \"idx_articles_status\" ON \"articles\" (status)"
        );

        let step_create_unique_idx = MigrationStep::CreateIndex(IndexDefinition::new(
            "idx_articles_slug",
            "articles",
            vec!["slug".to_string()],
            true,
        ));
        assert_eq!(
            step_create_unique_idx.describe(),
            "CREATE UNIQUE INDEX \"idx_articles_slug\" ON \"articles\" (slug)"
        );

        let step_drop_idx = MigrationStep::DropIndex {
            name: "idx_articles_slug".to_string(),
        };
        assert_eq!(step_drop_idx.describe(), "DROP INDEX \"idx_articles_slug\"");
    }

    #[test]
    fn test_safety_policy_serde() {
        assert_eq!(
            serde_json::from_str::<SafetyPolicy>(r#""additiveOnly""#).unwrap(),
            SafetyPolicy::AdditiveOnly
        );
        assert_eq!(
            serde_json::from_str::<SafetyPolicy>(r#""AdditiveOnly""#).unwrap(),
            SafetyPolicy::AdditiveOnly
        );
        assert_eq!(
            serde_json::from_str::<SafetyPolicy>(r#""additive""#).unwrap(),
            SafetyPolicy::AdditiveOnly
        );
        assert_eq!(
            serde_json::from_str::<SafetyPolicy>(r#""additive_only""#).unwrap(),
            SafetyPolicy::AdditiveOnly
        );
        assert_eq!(
            serde_json::from_str::<SafetyPolicy>(r#""allowDestructive""#).unwrap(),
            SafetyPolicy::AllowDestructive
        );
        assert_eq!(
            serde_json::from_str::<SafetyPolicy>(r#""AllowDestructive""#).unwrap(),
            SafetyPolicy::AllowDestructive
        );
        assert_eq!(
            serde_json::from_str::<SafetyPolicy>(r#""destructive""#).unwrap(),
            SafetyPolicy::AllowDestructive
        );
        assert_eq!(
            serde_json::from_str::<SafetyPolicy>(r#""allow_destructive""#).unwrap(),
            SafetyPolicy::AllowDestructive
        );
    }
}
