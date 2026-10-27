//! Topological Migration Step Planner.
//!
//! Orders DDL `MigrationStep`s into a dependency-safe `MigrationPlan` using Kahn's algorithm
//! on foreign key relationships.
//!
//! Execution Phases:
//!
//! Destruction Phase (when allowed by policy):
//! 1. Drop Indexes
//! 2. Drop Link Tables (child tables referencing entity tables)
//! 3. Drop Entity Tables (base parent tables)
//! 4. Drop Columns
//!
//! Construction Phase:
//! 5. Create Tables (topologically ordered: referenced parent tables before referencing child tables)
//! 6. Add Columns
//! 7. Create Indexes

use std::collections::{BTreeMap, HashSet, VecDeque};

use thiserror::Error;

use super::model::{TableDefinition, TableKind};
use super::step::MigrationStep;

// ---------------------------------------------------------------------------
// Planner Errors & Migration Plan
// ---------------------------------------------------------------------------

/// Errors encountered during migration planning.
#[derive(Debug, Error, PartialEq, Eq)]
pub(super) enum PlannerError {
    #[error("circular dependency detected involving tables: {0:?}")]
    CircularDependency(Vec<String>),
}

/// An ordered execution plan of DDL statements.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct MigrationPlan {
    pub(super) steps: Vec<MigrationStep>,
}

impl MigrationPlan {
    #[must_use]
    pub(super) fn len(&self) -> usize {
        self.steps.len()
    }
}

// ---------------------------------------------------------------------------
// Public Planning Engine
// ---------------------------------------------------------------------------

/// Organizes and sorts raw diff steps into a topologically sound execution order.
///
/// Tables to be created are sorted using Kahn's algorithm to ensure referenced tables
/// exist before foreign keys are created. Tables to be dropped are ordered child-first.
///
/// # Errors
/// Returns [`PlannerError::CircularDependency`] if a cycle of foreign key references is detected.
pub(super) fn plan_migrations(raw_steps: Vec<MigrationStep>) -> Result<MigrationPlan, PlannerError> {
    let partitioned = partition_steps(raw_steps);
    let ordered_create_tables = sort_create_tables(partitioned.create_tables)?;

    let mut planned = Vec::new();

    // 1. Destruction phase
    planned.extend(partitioned.drop_indexes);
    planned.extend(partitioned.drop_link_tables);
    planned.extend(partitioned.drop_entity_tables);
    planned.extend(partitioned.drop_columns);

    // 2. Construction phase
    for table in ordered_create_tables {
        planned.push(MigrationStep::CreateTable(table));
    }
    planned.extend(partitioned.add_columns);
    planned.extend(partitioned.create_indexes);

    Ok(MigrationPlan { steps: planned })
}

// ---------------------------------------------------------------------------
// Private Helpers & Graph Ordering
// ---------------------------------------------------------------------------

#[derive(Default)]
struct PartitionedSteps {
    drop_indexes: Vec<MigrationStep>,
    drop_link_tables: Vec<MigrationStep>,
    drop_entity_tables: Vec<MigrationStep>,
    drop_columns: Vec<MigrationStep>,
    create_tables: Vec<TableDefinition>,
    add_columns: Vec<MigrationStep>,
    create_indexes: Vec<MigrationStep>,
}

fn partition_steps(raw_steps: Vec<MigrationStep>) -> PartitionedSteps {
    let mut partitioned = PartitionedSteps::default();

    for step in raw_steps {
        match step {
            MigrationStep::DropIndex { .. } => partitioned.drop_indexes.push(step),
            MigrationStep::DropTable { kind, .. } => match kind {
                TableKind::Link => partitioned.drop_link_tables.push(step),
                _ => partitioned.drop_entity_tables.push(step),
            },
            MigrationStep::DropColumn { .. } => partitioned.drop_columns.push(step),
            MigrationStep::CreateTable(table) => partitioned.create_tables.push(table),
            MigrationStep::AddColumn { .. } => partitioned.add_columns.push(step),
            MigrationStep::CreateIndex(_) => partitioned.create_indexes.push(step),
        }
    }

    partitioned
}

struct DependencyGraph {
    dependents: BTreeMap<String, Vec<String>>,
    in_degree: BTreeMap<String, usize>,
}

fn build_dependency_graph(tables: &[TableDefinition]) -> DependencyGraph {
    let mut table_names = HashSet::with_capacity(tables.len());
    let mut dependents = BTreeMap::new();
    let mut in_degree = BTreeMap::new();

    for table in tables {
        table_names.insert(table.name.clone());
        dependents.insert(table.name.clone(), Vec::new());
        in_degree.insert(table.name.clone(), 0);
    }

    for table in tables {
        for dep in table.referenced_table_names() {
            if dep != table.name && table_names.contains(dep) {
                if let Some(deps) = dependents.get_mut(dep) {
                    deps.push(table.name.clone());
                }
                if let Some(deg) = in_degree.get_mut(&table.name) {
                    *deg += 1;
                }
            }
        }
    }

    DependencyGraph {
        dependents,
        in_degree,
    }
}

fn sort_create_tables(tables: Vec<TableDefinition>) -> Result<Vec<TableDefinition>, PlannerError> {
    let total_tables = tables.len();
    if total_tables <= 1 {
        return Ok(tables);
    }

    let DependencyGraph {
        dependents,
        mut in_degree,
    } = build_dependency_graph(&tables);

    // Kahn's algorithm: start with tables that have in-degree 0 (no dependencies in this migration)
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(name, _)| name.clone())
        .collect();

    let mut ordered_names = Vec::with_capacity(total_tables);

    while let Some(name) = queue.pop_front() {
        if let Some(deps) = dependents.get(&name) {
            for dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dep.clone());
                    }
                }
            }
        }
        ordered_names.push(name);
    }

    if ordered_names.len() != total_tables {
        let cycles: Vec<String> = in_degree
            .into_iter()
            .filter(|(_, deg)| *deg > 0)
            .map(|(name, _)| name)
            .collect();
        return Err(PlannerError::CircularDependency(cycles));
    }

    let mut map: std::collections::HashMap<String, TableDefinition> = tables
        .into_iter()
        .map(|t| (t.name.clone(), t))
        .collect();

    let ordered = ordered_names
        .into_iter()
        .filter_map(|name| map.remove(&name))
        .collect();

    Ok(ordered)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::migration::dynamic::model::{
        ForeignKeyAction, ForeignKeyDefinition, IndexDefinition, TableDefinition, TableKind,
    };

    fn make_table_with_fk(name: &str, kind: TableKind, ref_table: Option<&str>) -> TableDefinition {
        let mut table = TableDefinition::new(name, kind);
        if let Some(target) = ref_table {
            table.foreign_keys.insert(ForeignKeyDefinition::new(
                format!("fk_{name}_{target}"),
                vec!["target_id".into()],
                target,
                vec!["id".into()],
                ForeignKeyAction::Cascade,
                ForeignKeyAction::NoAction,
            ));
        }
        table
    }

    #[test]
    fn test_topological_ordering_creates_entities_before_links_and_indexes() {
        let entity_table = make_table_with_fk("articles", TableKind::Entity, None);
        let link_table =
            make_table_with_fk("articles_tags_links", TableKind::Link, Some("articles"));
        let index_def = IndexDefinition::new(
            "idx_articles_title",
            "articles",
            vec!["title".into()],
            false,
        );

        let raw = vec![
            MigrationStep::CreateIndex(index_def),
            MigrationStep::CreateTable(link_table),
            MigrationStep::CreateTable(entity_table),
        ];

        let plan = plan_migrations(raw).unwrap();
        assert_eq!(plan.steps.len(), 3);

        // 1. Entity table first
        assert!(matches!(
            &plan.steps[0],
            MigrationStep::CreateTable(t) if t.name == "articles"
        ));

        // 2. Link table second (depends on articles)
        assert!(matches!(
            &plan.steps[1],
            MigrationStep::CreateTable(t) if t.name == "articles_tags_links"
        ));

        // 3. Index third
        assert!(matches!(&plan.steps[2], MigrationStep::CreateIndex(_)));
    }

    #[test]
    fn test_topological_ordering_linear_entity_dependencies() {
        // articles depends on authors via FK
        let authors = make_table_with_fk("authors", TableKind::Entity, None);
        let articles = make_table_with_fk("articles", TableKind::Entity, Some("authors"));

        // Provided in reverse order
        let raw = vec![
            MigrationStep::CreateTable(articles),
            MigrationStep::CreateTable(authors),
        ];

        let plan = plan_migrations(raw).unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert!(matches!(
            &plan.steps[0],
            MigrationStep::CreateTable(t) if t.name == "authors"
        ));
        assert!(matches!(
            &plan.steps[1],
            MigrationStep::CreateTable(t) if t.name == "articles"
        ));
    }

    #[test]
    fn test_circular_dependency_detected() {
        // t1 depends on t2, and t2 depends on t1
        let t1 = make_table_with_fk("t1", TableKind::Entity, Some("t2"));
        let t2 = make_table_with_fk("t2", TableKind::Entity, Some("t1"));

        let raw = vec![
            MigrationStep::CreateTable(t1),
            MigrationStep::CreateTable(t2),
        ];

        let err = plan_migrations(raw).unwrap_err();
        assert!(matches!(err, PlannerError::CircularDependency(tables) if tables.contains(&"t1".to_string()) && tables.contains(&"t2".to_string())));
    }
}
