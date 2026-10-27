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
pub enum SafetyPolicy {
    /// Non-destructive changes only: provisions missing tables, columns, and indexes.
    #[default]
    AdditiveOnly,
    /// Allows dropping unmanaged tables, columns, or indexes (for test tear-down or local resets).
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
