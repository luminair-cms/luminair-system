//! Schema Drift Detection and Diffing Engine.
//!
//! Compares `ActualSchema` against `DesiredSchema` and generates a list of `MigrationStep`s.
//! Enforces `SafetyPolicy` (`AdditiveOnly` vs `AllowDestructive`).

use thiserror::Error;

use super::model::{DatabaseSchema, TableDefinition};
use super::step::{MigrationStep, SafetyPolicy};

// ---------------------------------------------------------------------------
// Diff Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DiffError {
    #[error("destructive operation blocked by SafetyPolicy::AdditiveOnly: {operation}")]
    DestructiveOperationBlocked { operation: String },
}

// ---------------------------------------------------------------------------
// Public Diffing Engine
// ---------------------------------------------------------------------------

/// Computes the difference between the actual live schema and the desired schema AST.
///
/// # Errors
/// Returns [`DiffError`] if schema validation detects an irreconcilable diff violation.
pub fn compute_diff(
    actual: &DatabaseSchema,
    desired: &DatabaseSchema,
    policy: SafetyPolicy,
) -> Result<Vec<MigrationStep>, DiffError> {
    let mut steps = Vec::new();

    diff_desired_tables(actual, desired, policy, &mut steps);
    diff_removed_tables(actual, desired, policy, &mut steps);

    Ok(steps)
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn diff_desired_tables(
    actual: &DatabaseSchema,
    desired: &DatabaseSchema,
    policy: SafetyPolicy,
    steps: &mut Vec<MigrationStep>,
) {
    for desired_table in &desired.tables {
        match actual.find_table(&desired_table.name) {
            None => {
                steps.push(MigrationStep::CreateTable(desired_table.clone()));
                for idx in &desired_table.indexes {
                    steps.push(MigrationStep::CreateIndex(idx.clone()));
                }
            }
            Some(actual_table) => {
                diff_table_columns(actual_table, desired_table, policy, steps);
                diff_table_indexes(actual_table, desired_table, policy, steps);
            }
        }
    }
}

fn diff_table_columns(
    actual_table: &TableDefinition,
    desired_table: &TableDefinition,
    policy: SafetyPolicy,
    steps: &mut Vec<MigrationStep>,
) {
    // 1. Add missing columns
    for desired_col in &desired_table.columns {
        if actual_table.find_column(&desired_col.name).is_none() {
            steps.push(MigrationStep::AddColumn {
                table: desired_table.name.clone(),
                column: desired_col.clone(),
            });
        }
    }

    // 2. Detect removed columns if destructive mode allowed
    for actual_col in &actual_table.columns {
        if desired_table.find_column(&actual_col.name).is_none() {
            match policy {
                SafetyPolicy::AdditiveOnly => {}
                SafetyPolicy::AllowDestructive => {
                    steps.push(MigrationStep::DropColumn {
                        table: desired_table.name.clone(),
                        column: actual_col.name.clone(),
                    });
                }
            }
        }
    }
}

fn diff_table_indexes(
    actual_table: &TableDefinition,
    desired_table: &TableDefinition,
    policy: SafetyPolicy,
    steps: &mut Vec<MigrationStep>,
) {
    // 1. Add missing indexes
    for desired_idx in &desired_table.indexes {
        if actual_table.find_index(&desired_idx.name).is_none() {
            steps.push(MigrationStep::CreateIndex(desired_idx.clone()));
        }
    }

    // 2. Detect removed indexes
    for actual_idx in &actual_table.indexes {
        if desired_table.find_index(&actual_idx.name).is_none() {
            match policy {
                SafetyPolicy::AdditiveOnly => {}
                SafetyPolicy::AllowDestructive => {
                    steps.push(MigrationStep::DropIndex {
                        name: actual_idx.name.clone(),
                        table: actual_table.name.clone(),
                    });
                }
            }
        }
    }
}

fn diff_removed_tables(
    actual: &DatabaseSchema,
    desired: &DatabaseSchema,
    policy: SafetyPolicy,
    steps: &mut Vec<MigrationStep>,
) {
    for actual_table in &actual.tables {
        if desired.find_table(&actual_table.name).is_none() {
            match policy {
                SafetyPolicy::AdditiveOnly => {}
                SafetyPolicy::AllowDestructive => {
                    steps.push(MigrationStep::DropTable {
                        name: actual_table.name.clone(),
                        kind: actual_table.kind,
                    });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::migration::dynamic::model::{
        ColumnDefinition, SqlColumnType, TableKind,
    };

    #[test]
    fn test_compute_diff_creates_table_when_missing() {
        let actual = DatabaseSchema::new();
        let mut desired = DatabaseSchema::new();
        let table = TableDefinition::new("articles", TableKind::Entity, false);
        desired.insert_table(table.clone());

        let steps = compute_diff(&actual, &desired, SafetyPolicy::AdditiveOnly).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0], MigrationStep::CreateTable(table));
    }

    #[test]
    fn test_compute_diff_adds_column_when_table_exists() {
        let mut actual = DatabaseSchema::new();
        let mut table_actual = TableDefinition::new("articles", TableKind::Entity, false);
        table_actual.columns.insert(ColumnDefinition::new(
            "id",
            SqlColumnType::Uuid,
            false,
            true,
        ));
        actual.insert_table(table_actual);

        let mut desired = DatabaseSchema::new();
        let mut table_desired = TableDefinition::new("articles", TableKind::Entity, false);
        table_desired.columns.insert(ColumnDefinition::new(
            "id",
            SqlColumnType::Uuid,
            false,
            true,
        ));
        table_desired.columns.insert(ColumnDefinition::new(
            "title",
            SqlColumnType::Text,
            false,
            false,
        ));
        desired.insert_table(table_desired);

        let steps = compute_diff(&actual, &desired, SafetyPolicy::AdditiveOnly).unwrap();
        assert_eq!(steps.len(), 1);
        assert!(matches!(
            &steps[0],
            MigrationStep::AddColumn { table, column } if table == "articles" && column.name == "title"
        ));
    }

    #[test]
    fn test_compute_diff_additive_policy_ignores_extra_actual_tables() {
        let mut actual = DatabaseSchema::new();
        actual.insert_table(TableDefinition::new("old_table", TableKind::Entity, false));

        let desired = DatabaseSchema::new();
        let steps = compute_diff(&actual, &desired, SafetyPolicy::AdditiveOnly).unwrap();
        assert!(steps.is_empty());
    }

    #[test]
    fn test_compute_diff_destructive_policy_drops_extra_actual_tables() {
        let mut actual = DatabaseSchema::new();
        actual.insert_table(TableDefinition::new("old_table", TableKind::Entity, false));

        let desired = DatabaseSchema::new();
        let steps = compute_diff(&actual, &desired, SafetyPolicy::AllowDestructive).unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(
            steps[0],
            MigrationStep::DropTable {
                name: "old_table".into(),
                kind: TableKind::Entity
            }
        );
    }
}
