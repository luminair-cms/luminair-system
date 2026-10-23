//! Topological Migration Step Planner.
//!
//! Orders DDL `MigrationStep`s into a dependency-safe `MigrationPlan`:
//!
//! Destruction Phase (when allowed by policy):
//! 1. Drop Indexes
//! 2. Drop Link Tables (referencing entity tables)
//! 3. Drop Entity Tables (base tables)
//! 4. Drop Columns
//!
//! Construction Phase:
//! 5. Create Entity Tables (base tables with primary keys)
//! 6. Create Link Tables (referencing entity tables via foreign keys)
//! 7. Add Columns
//! 8. Create Indexes

use super::diff::MigrationStep;
use super::model::TableKind;

// ---------------------------------------------------------------------------
// Migration Plan
// ---------------------------------------------------------------------------

/// An ordered execution plan of DDL statements.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MigrationPlan {
    pub steps: Vec<MigrationStep>,
}

impl MigrationPlan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }
}

// ---------------------------------------------------------------------------
// Public Planning Engine
// ---------------------------------------------------------------------------

/// Organizes and sorts raw diff steps into a topologically sound execution order.
pub fn plan_migrations(raw_steps: Vec<MigrationStep>) -> MigrationPlan {
    let mut drop_indexes = Vec::new();
    let mut drop_link_tables = Vec::new();
    let mut drop_entity_tables = Vec::new();
    let mut drop_columns = Vec::new();

    let mut create_entity_tables = Vec::new();
    let mut create_link_tables = Vec::new();
    let mut add_columns = Vec::new();
    let mut create_indexes = Vec::new();

    for step in raw_steps {
        match step {
            MigrationStep::DropIndex { .. } => drop_indexes.push(step),
            MigrationStep::DropTable { kind, .. } => match kind {
                TableKind::Link => drop_link_tables.push(step),
                _ => drop_entity_tables.push(step),
            },
            MigrationStep::DropColumn { .. } => drop_columns.push(step),
            MigrationStep::CreateTable(ref table) => match table.kind {
                TableKind::Link => create_link_tables.push(step),
                _ => create_entity_tables.push(step),
            },
            MigrationStep::AddColumn { .. } => add_columns.push(step),
            MigrationStep::CreateIndex(_) => create_indexes.push(step),
        }
    }

    let mut planned = Vec::new();
    planned.extend(drop_indexes);
    planned.extend(drop_link_tables);
    planned.extend(drop_entity_tables);
    planned.extend(drop_columns);

    planned.extend(create_entity_tables);
    planned.extend(create_link_tables);
    planned.extend(add_columns);
    planned.extend(create_indexes);

    MigrationPlan { steps: planned }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::migration::dynamic::model::{
        IndexDefinition, TableDefinition, TableKind,
    };

    #[test]
    fn test_topological_ordering_creates_entities_before_links_and_indexes() {
        let entity_table = TableDefinition::new("articles", TableKind::Entity, false);
        let link_table = TableDefinition::new("articles_tags_links", TableKind::Link, false);
        let index_def = IndexDefinition::new("idx_articles_title", "articles", vec!["title".into()], false);

        let raw = vec![
            MigrationStep::CreateIndex(index_def),
            MigrationStep::CreateTable(link_table),
            MigrationStep::CreateTable(entity_table),
        ];

        let plan = plan_migrations(raw);
        assert_eq!(plan.steps.len(), 3);

        // 1. Entity table first
        assert!(matches!(
            &plan.steps[0],
            MigrationStep::CreateTable(t) if t.name == "articles"
        ));

        // 2. Link table second
        assert!(matches!(
            &plan.steps[1],
            MigrationStep::CreateTable(t) if t.name == "articles_tags_links"
        ));

        // 3. Index third
        assert!(matches!(&plan.steps[2], MigrationStep::CreateIndex(_)));
    }
}
