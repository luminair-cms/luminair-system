//! Desired Database Schema Projection Builder.
//!
//! Transforms the domain `SchemaRegistry` into a normalized `DatabaseSchema` AST:
//! - Entity wide relational tables with standard lifecycle audit columns conforming to ADR-004.
//! - B-Tree indexes for lifecycle ordering and unique constraints.
//! - Relational junction link tables for relations with cascading foreign keys.

use domain::schema::{DocumentType, RelationDefinition, SchemaRegistry};

use super::model::{
    ColumnDefinition, DatabaseSchema, ForeignKeyAction, ForeignKeyDefinition, IndexDefinition,
    SqlColumnType, TableDefinition, TableKind,
};
use crate::persistence::naming::{
    COL_CREATED_AT, COL_CREATED_BY, COL_DISPLAY_TITLE, COL_DISPLAY_VALUES, COL_HAS_PENDING_DRAFT,
    COL_ID, COL_ORDER_INDEX, COL_PUBLISHED_AT, COL_PUBLISHED_BY, COL_REVISION, COL_SOURCE_ID,
    COL_TARGET_ID, COL_UPDATED_AT, COL_UPDATED_BY, column_name, junction_table_name,
    kebab_to_snake, table_name_for_document_type,
};

// ---------------------------------------------------------------------------
// Public Projection Builder
// ---------------------------------------------------------------------------

/// Builds the desired `DatabaseSchema` AST projection from the domain `SchemaRegistry`.
#[must_use]
pub(super) fn build_desired_schema(registry: &SchemaRegistry) -> DatabaseSchema {
    let mut schema = DatabaseSchema::new();

    for doc_type in registry.iterate() {
        let (entity_table, link_tables) = build_document_type_tables(doc_type, registry);
        schema.insert_table(entity_table);
        for link_table in link_tables {
            schema.insert_table(link_table);
        }
    }

    schema
}

/// Builds the desired table definitions for a single `DocumentType`.
fn build_document_type_tables(
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
) -> (TableDefinition, Vec<TableDefinition>) {
    let entity_table = build_entity_table(doc_type);
    let link_tables = build_link_tables(doc_type, registry);
    (entity_table, link_tables)
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn build_entity_table(doc_type: &DocumentType) -> TableDefinition {
    let table_name = table_name_for_document_type(doc_type);
    let mut table = TableDefinition::new(&table_name, TableKind::Entity);

    append_standard_lifecycle_columns(&mut table);
    append_user_field_columns(&mut table, doc_type);
    append_trailing_audit_columns(&mut table);
    append_standard_indexes(&mut table, &table_name, doc_type);

    table
}

fn append_standard_lifecycle_columns(table: &mut TableDefinition) {
    table.columns.insert(ColumnDefinition {
        name: COL_ID.into(),
        data_type: SqlColumnType::Uuid,
        nullable: false,
        is_primary_key: true,
        default_value: None,
    });

    table.columns.insert(ColumnDefinition {
        name: COL_HAS_PENDING_DRAFT.into(),
        data_type: SqlColumnType::Boolean,
        nullable: false,
        is_primary_key: false,
        default_value: Some("FALSE".into()),
    });

    table.columns.insert(ColumnDefinition {
        name: COL_DISPLAY_VALUES.into(),
        data_type: SqlColumnType::Jsonb,
        nullable: false,
        is_primary_key: false,
        default_value: Some("'{}'::jsonb".into()),
    });

    table.columns.insert(ColumnDefinition {
        name: COL_DISPLAY_TITLE.into(),
        data_type: SqlColumnType::Varchar(Some(255)),
        nullable: false,
        is_primary_key: false,
        default_value: None,
    });
}

fn append_user_field_columns(table: &mut TableDefinition, doc_type: &DocumentType) {
    for field in &doc_type.fields {
        let col_name = column_name(&field.id);
        let data_type = SqlColumnType::from(field.field_type);
        table.columns.insert(ColumnDefinition {
            name: col_name,
            data_type,
            nullable: !field.required,
            is_primary_key: false,
            default_value: None,
        });
    }
}

fn append_trailing_audit_columns(table: &mut TableDefinition) {
    table.columns.insert(ColumnDefinition {
        name: COL_REVISION.into(),
        data_type: SqlColumnType::Integer,
        nullable: false,
        is_primary_key: false,
        default_value: Some("1".into()),
    });

    table.columns.insert(ColumnDefinition {
        name: COL_PUBLISHED_AT.into(),
        data_type: SqlColumnType::Timestamptz,
        nullable: false,
        is_primary_key: false,
        default_value: None,
    });

    table.columns.insert(ColumnDefinition {
        name: COL_PUBLISHED_BY.into(),
        data_type: SqlColumnType::Varchar(Some(255)),
        nullable: false,
        is_primary_key: false,
        default_value: None,
    });

    table.columns.insert(ColumnDefinition {
        name: COL_CREATED_AT.into(),
        data_type: SqlColumnType::Timestamptz,
        nullable: false,
        is_primary_key: false,
        default_value: None,
    });

    table.columns.insert(ColumnDefinition {
        name: COL_CREATED_BY.into(),
        data_type: SqlColumnType::Varchar(Some(255)),
        nullable: false,
        is_primary_key: false,
        default_value: None,
    });

    table.columns.insert(ColumnDefinition {
        name: COL_UPDATED_AT.into(),
        data_type: SqlColumnType::Timestamptz,
        nullable: false,
        is_primary_key: false,
        default_value: None,
    });

    table.columns.insert(ColumnDefinition {
        name: COL_UPDATED_BY.into(),
        data_type: SqlColumnType::Varchar(Some(255)),
        nullable: false,
        is_primary_key: false,
        default_value: None,
    });
}

fn append_standard_indexes(table: &mut TableDefinition, table_name: &str, doc_type: &DocumentType) {
    table.indexes.insert(IndexDefinition::new(
        format!("idx_{table_name}_{COL_PUBLISHED_AT}"),
        table_name,
        vec![COL_PUBLISHED_AT.into()],
        false,
    ));

    table.indexes.insert(IndexDefinition::new(
        format!("idx_{table_name}_{COL_UPDATED_AT}"),
        table_name,
        vec![COL_UPDATED_AT.into()],
        false,
    ));

    for field in &doc_type.fields {
        if field.unique {
            let col = column_name(&field.id);
            table.indexes.insert(IndexDefinition::new(
                format!("uq_{table_name}_{col}"),
                table_name,
                vec![col],
                true,
            ));
        }
    }
}

fn build_link_tables(doc_type: &DocumentType, registry: &SchemaRegistry) -> Vec<TableDefinition> {
    let source_table = table_name_for_document_type(doc_type);
    let mut link_tables = Vec::new();

    for relation in &doc_type.relations {
        let j_table_name = junction_table_name(&source_table, &relation.id);
        let target_table = resolve_target_table(relation, registry);
        let mut link = TableDefinition::new(&j_table_name, TableKind::Link);

        link.columns.insert(ColumnDefinition {
            name: COL_SOURCE_ID.into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
        });

        link.columns.insert(ColumnDefinition {
            name: COL_TARGET_ID.into(),
            data_type: SqlColumnType::Uuid,
            nullable: false,
            is_primary_key: true,
            default_value: None,
        });

        link.columns.insert(ColumnDefinition {
            name: COL_ORDER_INDEX.into(),
            data_type: SqlColumnType::Integer,
            nullable: false,
            is_primary_key: false,
            default_value: Some("0".into()),
        });

        link.foreign_keys.insert(ForeignKeyDefinition::new(
            format!("fk_{j_table_name}_source"),
            vec![COL_SOURCE_ID.into()],
            &source_table,
            vec![COL_ID.into()],
            ForeignKeyAction::Cascade,
            ForeignKeyAction::NoAction,
        ));

        link.foreign_keys.insert(ForeignKeyDefinition::new(
            format!("fk_{j_table_name}_target"),
            vec![COL_TARGET_ID.into()],
            &target_table,
            vec![COL_ID.into()],
            ForeignKeyAction::Cascade,
            ForeignKeyAction::NoAction,
        ));

        link.indexes.insert(IndexDefinition::new(
            format!("idx_{j_table_name}_{COL_TARGET_ID}"),
            &j_table_name,
            vec![COL_TARGET_ID.into()],
            false,
        ));

        link_tables.push(link);
    }

    link_tables
}

fn resolve_target_table(relation: &RelationDefinition, registry: &SchemaRegistry) -> String {
    registry.get(&relation.target).map_or_else(
        || kebab_to_snake(relation.target.as_ref()),
        table_name_for_document_type,
    )
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
        FieldDefinition, FieldType, PrimitiveType, RelationType,
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
    fn test_build_desired_schema_generates_entity_and_link_tables() {
        let tag_rel = RelationDefinition {
            id: AttributeId::try_new("tags").unwrap(),
            relation_type: RelationType::HasMany,
            target: DocumentTypeId::try_new("tag").unwrap(),
        };
        let slug_field = FieldDefinition {
            id: AttributeId::try_new("slug").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Uid),
            unique: true,
            required: true,
            constraints: HashSet::new(),
        };

        let article_type = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![slug_field],
            vec![tag_rel],
        );
        let tag_type = make_test_doc_type("tag", "tags", DocumentKind::Collection, vec![], vec![]);

        let registry = SchemaRegistry::new(vec![article_type, tag_type]);
        let schema = build_desired_schema(&registry);

        assert_eq!(schema.tables.len(), 3); // articles, tags, articles_tags_links

        let articles = schema.find_table("articles").unwrap();
        assert_eq!(articles.kind, TableKind::Entity);
        assert!(articles.find_column("id").unwrap().is_primary_key);
        assert!(articles.find_column("has_pending_draft").is_some());
        assert!(articles.find_column("display_title").is_some());
        assert!(articles.find_index("idx_articles_published_at").is_some());
        assert!(articles.find_index("uq_articles_slug").is_some());

        let link = schema.find_table("articles_tags_links").unwrap();
        assert_eq!(link.kind, TableKind::Link);
        assert!(link.find_column("source_id").is_some());
        assert!(link.find_column("target_id").is_some());
        assert!(link.find_column("order_index").is_some());
        assert!(
            link.find_foreign_key("fk_articles_tags_links_source")
                .is_some()
        );
        assert!(
            link.find_foreign_key("fk_articles_tags_links_target")
                .is_some()
        );
        assert!(
            link.find_index("idx_articles_tags_links_target_id")
                .is_some()
        );
    }
}
