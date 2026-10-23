//! Dynamic additive schema synchronizer for Luminair document types.
//!
//! Generates and applies additive DDL statements (`CREATE TABLE IF NOT EXISTS`,
//! `ALTER TABLE ... ADD COLUMN IF NOT EXISTS`, indexes, and relational junction tables)
//! conforming to PostgreSQL and AWS Aurora DSQL without transaction blocks.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use domain::schema::{DocumentType, FieldDefinition, RelationDefinition, SchemaRegistry};
use sqlx::PgPool;

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{
    column_name, junction_table_name, kebab_to_snake, sql_type_for_field,
    table_name_for_document_type, COL_CREATED_AT, COL_CREATED_BY, COL_DISPLAY_TITLE,
    COL_DISPLAY_VALUES, COL_HAS_PENDING_DRAFT, COL_ID, COL_ORDER_INDEX, COL_PUBLISHED_AT,
    COL_PUBLISHED_BY, COL_REVISION, COL_SOURCE_ID, COL_TARGET_ID, COL_UPDATED_AT, COL_UPDATED_BY,
};
use crate::schema_loader::load_schemas_from_dir;

// ---------------------------------------------------------------------------
// Public Types
// ---------------------------------------------------------------------------

/// Safety policy governing dynamic database schema synchronization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SafetyPolicy {
    /// Strictly non-destructive changes: create missing tables, add missing columns,
    /// provision missing junction tables and indexes. Existing tables/columns are never dropped.
    #[default]
    AdditiveOnly,
}

/// Statistics and record of statements executed during dynamic schema synchronization.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncResult {
    pub executed_statements: Vec<String>,
    pub tables_created: usize,
    pub columns_added: usize,
    pub indexes_created: usize,
}

impl SyncResult {
    pub fn record_table_created(&mut self, statement: String) {
        self.tables_created += 1;
        self.executed_statements.push(statement);
    }

    pub fn record_column_added(&mut self, statement: String) {
        self.columns_added += 1;
        self.executed_statements.push(statement);
    }

    pub fn record_index_created(&mut self, statement: String) {
        self.indexes_created += 1;
        self.executed_statements.push(statement);
    }
}

// ---------------------------------------------------------------------------
// Public API Functions
// ---------------------------------------------------------------------------

/// Loads schema definitions from `schema_dir` and synchronizes the target database pool.
pub async fn sync_schemas_from_dir(
    pool: &PgPool,
    schema_dir: &Path,
    policy: SafetyPolicy,
) -> Result<SyncResult, PersistenceError> {
    let (_config, registry) = load_schemas_from_dir(schema_dir)?;
    sync_schemas(pool, &registry, policy).await
}

/// Synchronizes a live database pool to match the given `SchemaRegistry`.
///
/// Queries existing tables and columns via `information_schema`, determines missing
/// entities, and executes additive DDL statements outside transaction blocks.
pub async fn sync_schemas(
    pool: &PgPool,
    registry: &SchemaRegistry,
    policy: SafetyPolicy,
) -> Result<SyncResult, PersistenceError> {
    let existing_tables = fetch_existing_tables(pool).await?;
    let existing_columns = fetch_existing_columns(pool).await?;
    let mut result = SyncResult::default();

    for doc_type in registry.iterate() {
        sync_document_type(
            pool,
            doc_type,
            registry,
            &existing_tables,
            &existing_columns,
            policy,
            &mut result,
        )
        .await?;
    }

    Ok(result)
}

/// Generates all additive DDL statements for all document types in the registry.
///
/// Orders statements safely: wide tables first, column additions second,
/// standard indexes third, and relational junction tables and indexes last.
pub fn generate_ddl_for_registry(registry: &SchemaRegistry) -> Vec<String> {
    let mut wide_tables = Vec::new();
    let mut indexes = Vec::new();
    let mut junction_tables = Vec::new();

    for doc_type in registry.iterate() {
        let table_name = table_name_for_document_type(doc_type);
        wide_tables.push(build_create_table_ddl(doc_type));

        for idx in build_table_indexes_ddl(&table_name) {
            indexes.push(idx);
        }

        for junction_ddl in build_junction_tables_ddl(doc_type, registry) {
            junction_tables.push(junction_ddl);
        }
    }

    let mut statements = wide_tables;
    statements.extend(indexes);
    statements.extend(junction_tables);
    statements
}

/// Generates DDL statements for a single `DocumentType`.
pub fn generate_ddl_for_document_type(
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
) -> Vec<String> {
    let table_name = table_name_for_document_type(doc_type);
    let mut statements = vec![build_create_table_ddl(doc_type)];

    for idx in build_table_indexes_ddl(&table_name) {
        statements.push(idx);
    }

    for junction_ddl in build_junction_tables_ddl(doc_type, registry) {
        statements.push(junction_ddl);
    }

    statements
}

/// Generates the `CREATE TABLE IF NOT EXISTS` statement for a document type's wide table.
pub fn build_create_table_ddl(doc_type: &DocumentType) -> String {
    let table_name = table_name_for_document_type(doc_type);
    let mut lines = Vec::new();

    lines.push(format!("    {COL_ID} UUID PRIMARY KEY,"));
    lines.push(format!("    {COL_HAS_PENDING_DRAFT} BOOLEAN NOT NULL DEFAULT FALSE,"));
    lines.push(format!("    {COL_DISPLAY_VALUES} JSONB NOT NULL DEFAULT '{{}}'::jsonb,"));
    lines.push(format!("    {COL_DISPLAY_TITLE} VARCHAR(255) NOT NULL,"));

    for field in &doc_type.fields {
        lines.push(format!("    {},", build_field_column_def(field)));
    }

    lines.push(format!("    {COL_REVISION} INT NOT NULL DEFAULT 1,"));
    lines.push(format!("    {COL_PUBLISHED_AT} TIMESTAMPTZ NOT NULL,"));
    lines.push(format!("    {COL_PUBLISHED_BY} VARCHAR(255) NOT NULL,"));
    lines.push(format!("    {COL_CREATED_AT} TIMESTAMPTZ NOT NULL,"));
    lines.push(format!("    {COL_CREATED_BY} VARCHAR(255) NOT NULL,"));
    lines.push(format!("    {COL_UPDATED_AT} TIMESTAMPTZ NOT NULL,"));
    lines.push(format!("    {COL_UPDATED_BY} VARCHAR(255) NOT NULL"));

    format!("CREATE TABLE IF NOT EXISTS {table_name} (\n{}\n);", lines.join("\n"))
}

/// Generates the `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` statement for an additive field.
pub fn build_add_column_ddl(table_name: &str, field: &FieldDefinition) -> String {
    let col = column_name(&field.id);
    let sql_type = sql_type_for_field(&field.field_type);
    format!("ALTER TABLE {table_name} ADD COLUMN IF NOT EXISTS {col} {sql_type};")
}

/// Generates the default B-tree index creation statements for a wide table.
pub fn build_table_indexes_ddl(table_name: &str) -> Vec<String> {
    vec![
        format!("CREATE INDEX IF NOT EXISTS idx_{table_name}_{COL_PUBLISHED_AT} ON {table_name} ({COL_PUBLISHED_AT} DESC);"),
        format!("CREATE INDEX IF NOT EXISTS idx_{table_name}_{COL_UPDATED_AT} ON {table_name} ({COL_UPDATED_AT} DESC);"),
    ]
}

/// Generates junction table and index DDL statements for all relations of a document type.
pub fn build_junction_tables_ddl(
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
) -> Vec<String> {
    let source_table = table_name_for_document_type(doc_type);
    let mut ddl = Vec::new();

    for relation in &doc_type.relations {
        let j_table = junction_table_name(&source_table, &relation.id);
        let target_table = resolve_target_table(relation, registry);

        ddl.push(build_junction_table_ddl(&source_table, &target_table, &j_table));
        ddl.push(build_junction_index_ddl(&j_table));
    }

    ddl
}

// ---------------------------------------------------------------------------
// Private Synchronizer Orchestrator & Database Helpers
// ---------------------------------------------------------------------------

async fn sync_document_type(
    pool: &PgPool,
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
    existing_tables: &HashSet<String>,
    existing_columns: &HashMap<String, HashSet<String>>,
    _policy: SafetyPolicy,
    result: &mut SyncResult,
) -> Result<(), PersistenceError> {
    let table_name = table_name_for_document_type(doc_type);

    if !existing_tables.contains(&table_name) {
        let create_sql = build_create_table_ddl(doc_type);
        execute_statement(pool, &create_sql).await?;
        result.record_table_created(create_sql);

        for idx_sql in build_table_indexes_ddl(&table_name) {
            execute_statement(pool, &idx_sql).await?;
            result.record_index_created(idx_sql);
        }
    } else {
        sync_missing_columns(pool, doc_type, &table_name, existing_columns, result).await?;
    }

    sync_junction_tables(pool, doc_type, registry, existing_tables, result).await?;
    Ok(())
}

async fn sync_missing_columns(
    pool: &PgPool,
    doc_type: &DocumentType,
    table_name: &str,
    existing_columns: &HashMap<String, HashSet<String>>,
    result: &mut SyncResult,
) -> Result<(), PersistenceError> {
    let table_cols = existing_columns.get(table_name);
    for field in &doc_type.fields {
        let col = column_name(&field.id);
        let exists = table_cols.is_some_and(|c| c.contains(&col));
        if !exists {
            let alter_sql = build_add_column_ddl(table_name, field);
            execute_statement(pool, &alter_sql).await?;
            result.record_column_added(alter_sql);
        }
    }
    Ok(())
}

async fn sync_junction_tables(
    pool: &PgPool,
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
    existing_tables: &HashSet<String>,
    result: &mut SyncResult,
) -> Result<(), PersistenceError> {
    let source_table = table_name_for_document_type(doc_type);
    for relation in &doc_type.relations {
        let j_table = junction_table_name(&source_table, &relation.id);
        if !existing_tables.contains(&j_table) {
            let target_table = resolve_target_table(relation, registry);
            let create_sql = build_junction_table_ddl(&source_table, &target_table, &j_table);
            execute_statement(pool, &create_sql).await?;
            result.record_table_created(create_sql);

            let idx_sql = build_junction_index_ddl(&j_table);
            execute_statement(pool, &idx_sql).await?;
            result.record_index_created(idx_sql);
        }
    }
    Ok(())
}

fn build_field_column_def(field: &FieldDefinition) -> String {
    let col = column_name(&field.id);
    let sql_type = sql_type_for_field(&field.field_type);
    let mut def = format!("{col} {sql_type}");
    if field.required {
        def.push_str(" NOT NULL");
    }
    if field.unique {
        def.push_str(" UNIQUE");
    }
    def
}

fn build_junction_table_ddl(
    source_table: &str,
    target_table: &str,
    junction_table: &str,
) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {junction_table} (\n    {COL_SOURCE_ID} UUID NOT NULL REFERENCES {source_table}({COL_ID}) ON DELETE CASCADE,\n    {COL_TARGET_ID} UUID NOT NULL REFERENCES {target_table}({COL_ID}) ON DELETE CASCADE,\n    {COL_ORDER_INDEX} INT NOT NULL DEFAULT 0,\n    PRIMARY KEY ({COL_SOURCE_ID}, {COL_TARGET_ID})\n);"
    )
}

fn build_junction_index_ddl(junction_table: &str) -> String {
    format!("CREATE INDEX IF NOT EXISTS idx_{junction_table}_{COL_TARGET_ID} ON {junction_table} ({COL_TARGET_ID});")
}

fn resolve_target_table(relation: &RelationDefinition, registry: &SchemaRegistry) -> String {
    registry
        .get(&relation.target)
        .map(table_name_for_document_type)
        .unwrap_or_else(|| kebab_to_snake(relation.target.as_ref()))
}

async fn fetch_existing_tables(pool: &PgPool) -> Result<HashSet<String>, PersistenceError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT table_name::TEXT FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

async fn fetch_existing_columns(
    pool: &PgPool,
) -> Result<HashMap<String, HashSet<String>>, PersistenceError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT table_name::TEXT, column_name::TEXT FROM information_schema.columns WHERE table_schema = 'public'",
    )
    .fetch_all(pool)
    .await?;
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for (tbl, col) in rows {
        map.entry(tbl).or_default().insert(col);
    }
    Ok(map)
}

async fn execute_statement(pool: &PgPool, sql: &str) -> Result<(), PersistenceError> {
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(pool).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::common::DisplayName;
    use domain::schema::{
        AttributeId, DocumentKind, DocumentTypeId, DocumentTypeInfo, DocumentTypeOptions,
        FieldType, IntegerSize, PrimitiveType, RelationType,
    };

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
    fn test_build_create_table_ddl_standard_columns() {
        let title_field = FieldDefinition {
            id: AttributeId::try_new("title").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            unique: false,
            required: true,
            constraints: HashSet::new(),
        };
        let slug_field = FieldDefinition {
            id: AttributeId::try_new("slug").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Uid),
            unique: true,
            required: true,
            constraints: HashSet::new(),
        };

        let doc_type = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![slug_field, title_field],
            vec![],
        );

        let ddl = build_create_table_ddl(&doc_type);
        assert!(ddl.starts_with("CREATE TABLE IF NOT EXISTS articles ("));
        assert!(ddl.contains("id UUID PRIMARY KEY"));
        assert!(ddl.contains("has_pending_draft BOOLEAN NOT NULL DEFAULT FALSE"));
        assert!(ddl.contains("display_values JSONB NOT NULL DEFAULT '{}'::jsonb"));
        assert!(ddl.contains("display_title VARCHAR(255) NOT NULL"));
        assert!(ddl.contains("slug VARCHAR(255) NOT NULL UNIQUE"));
        assert!(ddl.contains("title TEXT NOT NULL"));
        assert!(ddl.contains("revision INT NOT NULL DEFAULT 1"));
        assert!(ddl.contains("published_at TIMESTAMPTZ NOT NULL"));
    }

    #[test]
    fn test_build_table_indexes_ddl() {
        let indexes = build_table_indexes_ddl("articles");
        assert_eq!(indexes.len(), 2);
        assert_eq!(
            indexes[0],
            "CREATE INDEX IF NOT EXISTS idx_articles_published_at ON articles (published_at DESC);"
        );
        assert_eq!(
            indexes[1],
            "CREATE INDEX IF NOT EXISTS idx_articles_updated_at ON articles (updated_at DESC);"
        );
    }

    #[test]
    fn test_build_junction_tables_ddl() {
        let tag_rel = RelationDefinition {
            id: AttributeId::try_new("tags").unwrap(),
            relation_type: RelationType::HasMany,
            target: DocumentTypeId::try_new("tag").unwrap(),
        };

        let doc_type = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![],
            vec![tag_rel],
        );

        let tag_type = make_test_doc_type("tag", "tags", DocumentKind::Collection, vec![], vec![]);
        let registry = SchemaRegistry::new(vec![doc_type.clone(), tag_type]);

        let junction_ddls = build_junction_tables_ddl(&doc_type, &registry);
        assert_eq!(junction_ddls.len(), 2);
        assert!(junction_ddls[0].contains("CREATE TABLE IF NOT EXISTS articles_tags_links ("));
        assert!(junction_ddls[0].contains("source_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE"));
        assert!(junction_ddls[0].contains("target_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE"));
        assert_eq!(
            junction_ddls[1],
            "CREATE INDEX IF NOT EXISTS idx_articles_tags_links_target_id ON articles_tags_links (target_id);"
        );
    }

    #[test]
    fn test_generate_ddl_for_registry_ordering() {
        let article_type = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![],
            vec![],
        );
        let site_settings = make_test_doc_type(
            "site-settings",
            "site-settings-plural",
            DocumentKind::Singleton,
            vec![],
            vec![],
        );

        let registry = SchemaRegistry::new(vec![article_type, site_settings]);

        let ddl = generate_ddl_for_registry(&registry);
        assert!(ddl.len() >= 6); // 2 tables + 4 indexes
        assert!(ddl[0].starts_with("CREATE TABLE IF NOT EXISTS"));
        assert!(ddl[1].starts_with("CREATE TABLE IF NOT EXISTS"));
        assert!(ddl[2].starts_with("CREATE INDEX IF NOT EXISTS"));
    }

    #[test]
    fn test_build_add_column_ddl() {
        let priority_field = FieldDefinition {
            id: AttributeId::try_new("priority").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            unique: false,
            required: false,
            constraints: HashSet::new(),
        };

        let alter_sql = build_add_column_ddl("articles", &priority_field);
        assert_eq!(
            alter_sql,
            "ALTER TABLE articles ADD COLUMN IF NOT EXISTS priority INTEGER;"
        );
    }
}
