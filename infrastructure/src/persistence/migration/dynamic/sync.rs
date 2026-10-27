//! Schema synchronization pipeline: Registry -> Desired AST -> Introspect -> Diff -> Plan -> DDL Execution.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use domain::schema::SchemaRegistry;

use super::builder::build_desired_schema;
use super::ddl::step_to_sql;
use super::diff::compute_diff;
use super::executor::execute_migration_plan;
use super::introspector::introspect_database_schema;
use super::model::DatabaseSchema;
use super::planner::{PlannerError, plan_migrations};
use super::step::{MigrationStep, SafetyPolicy};
use crate::persistence::error::PersistenceError;
use crate::schema_loader::load_system_context_from_dir;

// ---------------------------------------------------------------------------
// Result Types
// ---------------------------------------------------------------------------

/// The result summary of running schema synchronization against a live database.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSyncResult {
    pub executed_statements: Vec<String>,
    pub tables_created: usize,
    pub columns_added: usize,
    pub indexes_created: usize,
}

/// A single planned dynamic DDL step with human-readable operation summary and SQL statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedMigrationStep {
    pub step_number: usize,
    pub operation: String,
    pub sql: String,
}

// ---------------------------------------------------------------------------
// Public Synchronization Entry Points
// ---------------------------------------------------------------------------

/// Synchronizes a live database pool from a directory of schema JSON definitions.
///
/// # Errors
/// Returns [`PersistenceError`] if schema loading, diffing, or DDL execution fails.
pub async fn sync_dynamic_schemas_from_dir(
    pool: &PgPool,
    schema_dir: &Path,
    policy: SafetyPolicy,
) -> Result<SchemaSyncResult, PersistenceError> {
    let context = load_system_context_from_dir(schema_dir)?;
    sync_dynamic_schemas(pool, &context.schema, policy).await
}

/// Synchronizes a live database schema to match an in-memory `SchemaRegistry`.
///
/// Pipeline stages:
/// 1. Builds desired database schema AST projection from the `SchemaRegistry`
/// 2. Introspects current actual database schema from `information_schema`
/// 3. Computes schema diff adhering to `SafetyPolicy`
/// 4. Plans migration steps topologically (base entity tables before child link tables)
/// 5. Executes generated DDL via type-safe `SeaQuery` statements
///
/// # Errors
/// Returns [`PersistenceError`] if schema introspection, diff computation, or DDL execution fails.
pub async fn sync_dynamic_schemas(
    pool: &PgPool,
    registry: &SchemaRegistry,
    policy: SafetyPolicy,
) -> Result<SchemaSyncResult, PersistenceError> {
    let desired = build_desired_schema(registry);
    let actual = introspect_database_schema(pool).await?;
    let raw_steps = compute_diff(&actual, &desired, policy);

    if raw_steps.is_empty() {
        return Ok(SchemaSyncResult::default());
    }

    let plan = plan_migrations(raw_steps).map_err(|e| match e {
        PlannerError::CircularDependency(tables) => {
            PersistenceError::DependencyCycle(tables.join(", "))
        }
    })?;
    let summary = execute_migration_plan(pool, &plan).await?;
    let counts = calculate_step_counts(&plan.steps);

    Ok(SchemaSyncResult {
        executed_statements: summary.executed_statements,
        tables_created: counts.0,
        columns_added: counts.1,
        indexes_created: counts.2,
    })
}

/// Computes the planned dynamic schema migration steps without executing any DDL.
///
/// When `pool` is `Some`, introspects the live database schema to compute only the pending diff.
/// When `pool` is `None`, computes all planned steps against an empty database schema.
///
/// # Errors
/// Returns [`PersistenceError`] if database introspection or topological planning fails.
pub async fn preview_dynamic_schemas(
    pool: Option<&PgPool>,
    registry: &SchemaRegistry,
    policy: SafetyPolicy,
) -> Result<Vec<PlannedMigrationStep>, PersistenceError> {
    let desired = build_desired_schema(registry);
    let actual = match pool {
        Some(db_pool) => introspect_database_schema(db_pool).await?,
        None => DatabaseSchema::new(),
    };
    let raw_steps = compute_diff(&actual, &desired, policy);
    if raw_steps.is_empty() {
        return Ok(Vec::new());
    }

    let plan = plan_migrations(raw_steps).map_err(|e| match e {
        PlannerError::CircularDependency(tables) => {
            PersistenceError::DependencyCycle(tables.join(", "))
        }
    })?;

    let steps = plan
        .steps
        .iter()
        .enumerate()
        .map(|(idx, step)| PlannedMigrationStep {
            step_number: idx + 1,
            operation: step.describe(),
            sql: step_to_sql(step),
        })
        .collect();

    Ok(steps)
}

/// Generates all additive DDL statements for all document types in the registry without executing them.
#[must_use]
pub fn generate_ddl_for_registry(registry: &SchemaRegistry) -> Vec<String> {
    let desired = build_desired_schema(registry);
    let actual = DatabaseSchema::new();
    let raw_steps = compute_diff(&actual, &desired, SafetyPolicy::AdditiveOnly);
    let plan = plan_migrations(raw_steps).unwrap_or_default();
    plan.steps.iter().map(step_to_sql).collect()
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn calculate_step_counts(steps: &[MigrationStep]) -> (usize, usize, usize) {
    let mut tables = 0;
    let mut cols = 0;
    let mut idxs = 0;

    for step in steps {
        match step {
            MigrationStep::CreateTable(_) => tables += 1,
            MigrationStep::AddColumn { .. } => cols += 1,
            MigrationStep::CreateIndex(_) => idxs += 1,
            _ => {}
        }
    }

    (tables, cols, idxs)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::common::DisplayName;
    use domain::schema::{
        AttributeId, DocumentKind, DocumentType, DocumentTypeId, DocumentTypeInfo,
        DocumentTypeOptions, FieldDefinition, FieldType, IntegerSize, PrimitiveType,
        RelationDefinition, RelationType,
    };
    use std::collections::HashSet;

    fn make_test_doc_type(
        id: &str,
        plural: &str,
        kind: DocumentKind,
        fields: Vec<FieldDefinition>,
        relations: Vec<RelationDefinition>,
    ) -> DocumentType {
        let type_id = DocumentTypeId::try_new(id).unwrap();
        let plural_id = DocumentTypeId::try_new(plural).unwrap();
        DocumentType {
            id: type_id.clone(),
            kind,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Test").unwrap(),
                singular_name: type_id,
                plural_name: plural_id,
                description: None,
                display_fields: vec![],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: fields.into_iter().collect(),
            relations: relations.into_iter().collect(),
        }
    }

    #[test]
    fn test_generate_ddl_for_registry_pipeline() {
        let tag_rel = RelationDefinition {
            id: AttributeId::try_new("tags").unwrap(),
            relation_type: RelationType::HasMany,
            target: DocumentTypeId::try_new("tag").unwrap(),
        };
        let priority_field = FieldDefinition {
            id: AttributeId::try_new("priority").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            unique: false,
            required: false,
            constraints: HashSet::new(),
        };

        let article_type = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![priority_field],
            vec![tag_rel],
        );
        let tag_type = make_test_doc_type("tag", "tags", DocumentKind::Collection, vec![], vec![]);

        let registry = SchemaRegistry::new(vec![article_type, tag_type]);
        let ddl_statements = generate_ddl_for_registry(&registry);

        // Verify statements are ordered: Entity tables first, link tables second, indexes last
        assert!(!ddl_statements.is_empty());

        let create_articles = ddl_statements
            .iter()
            .find(|s| s.contains(r#"CREATE TABLE IF NOT EXISTS "articles""#));
        assert!(create_articles.is_some());
        assert!(
            create_articles
                .unwrap()
                .contains(r#""has_pending_draft" bool NOT NULL DEFAULT FALSE"#)
        );
        assert!(
            create_articles
                .unwrap()
                .contains(r#""display_values" jsonb NOT NULL DEFAULT '{}'::jsonb"#)
        );

        let create_links = ddl_statements
            .iter()
            .find(|s| s.contains(r#"CREATE TABLE IF NOT EXISTS "articles_tags_links""#));
        assert!(create_links.is_some());
        assert!(create_links.unwrap().contains(
            r#"FOREIGN KEY ("source_id") REFERENCES "articles" ("id") ON DELETE CASCADE"#
        ));

        let index_stmt = ddl_statements.iter().find(|s| {
            s.contains(r#"CREATE INDEX IF NOT EXISTS "idx_articles_tags_links_target_id""#)
        });
        assert!(index_stmt.is_some());
    }

    #[tokio::test]
    async fn test_preview_dynamic_schemas_offline() {
        let tag_rel = RelationDefinition {
            id: AttributeId::try_new("tags").unwrap(),
            relation_type: RelationType::HasMany,
            target: DocumentTypeId::try_new("tag").unwrap(),
        };
        let priority_field = FieldDefinition {
            id: AttributeId::try_new("priority").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            unique: false,
            required: false,
            constraints: HashSet::new(),
        };

        let article_type = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![priority_field],
            vec![tag_rel],
        );
        let tag_type = make_test_doc_type("tag", "tags", DocumentKind::Collection, vec![], vec![]);

        let registry = SchemaRegistry::new(vec![article_type, tag_type]);
        let steps = preview_dynamic_schemas(None, &registry, SafetyPolicy::AdditiveOnly)
            .await
            .unwrap();

        assert!(!steps.is_empty());
        for (i, step) in steps.iter().enumerate() {
            assert_eq!(step.step_number, i + 1);
            assert!(!step.operation.is_empty());
            assert!(!step.sql.is_empty());
        }

        let article_table_step = steps
            .iter()
            .find(|s| s.operation.starts_with("CREATE TABLE \"articles\""));
        assert!(article_table_step.is_some());
        assert!(
            article_table_step
                .unwrap()
                .sql
                .contains(r#"CREATE TABLE IF NOT EXISTS "articles""#)
        );

        let links_step = steps
            .iter()
            .find(|s| s.operation.starts_with("CREATE TABLE \"articles_tags_links\""));
        assert!(links_step.is_some());
        assert!(
            links_step
                .unwrap()
                .sql
                .contains(r#"CREATE TABLE IF NOT EXISTS "articles_tags_links""#)
        );

        let index_step = steps.iter().find(|s| s.operation.starts_with("CREATE INDEX"));
        assert!(index_step.is_some());
        assert!(
            index_step
                .unwrap()
                .sql
                .contains(r#"CREATE INDEX IF NOT EXISTS"#)
        );
    }
}
