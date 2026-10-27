//! Startup-precomputed zero-allocation schema naming registry and static Sea-Query identifier cache.

use indexmap::IndexMap;

use domain::schema::{
    AttributeId, DocumentType, DocumentTypeId, FieldType, RelationDefinition, RelationType,
    SchemaRegistry,
};

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{
    COL_CREATED_AT, COL_CREATED_BY, COL_DISPLAY_TITLE, COL_DISPLAY_VALUES, COL_HAS_PENDING_DRAFT,
    COL_ID, COL_PUBLISHED_AT, COL_PUBLISHED_BY, COL_REVISION, COL_UPDATED_AT, COL_UPDATED_BY,
    column_name, junction_table_name, kebab_to_snake, table_name_for_document_type,
};

// ---------------------------------------------------------------------------
// Public Types
// ---------------------------------------------------------------------------

/// Zero-allocation `Copy` wrapper around `&'static str` implementing `sea_query::Iden`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StaticIden(pub &'static str);

/// Precomputed column name and type metadata for a schema field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldColumnNaming {
    pub column_name: &'static str,
    pub column_iden: StaticIden,
    pub field_type: FieldType,
}

/// Precomputed table and junction metadata for a schema relation (both owning and inverse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationNaming {
    pub attribute_id: AttributeId,
    pub relation_type: RelationType,
    pub target_type_id: DocumentTypeId,
    pub target_table_name: &'static str,
    pub target_table_iden: StaticIden,
    /// Precomputed junction table name (`[owner_table]_[owner_rel]_links`), resolved for both owning and inverse relations.
    pub junction_table_name: &'static str,
    pub junction_table_iden: StaticIden,
}

/// Precomputed zero-allocation SQL identifiers and column slices for a single `DocumentType`.
#[derive(Debug, Clone)]
pub struct DocumentTypeNaming {
    pub document_type_id: DocumentTypeId,
    pub table_name: &'static str,
    pub table_iden: StaticIden,
    pub field_columns: IndexMap<AttributeId, FieldColumnNaming>,
    pub wide_upsert_columns: &'static [StaticIden],
    pub wide_update_columns: &'static [StaticIden],
    pub relations: IndexMap<AttributeId, RelationNaming>,
}

/// Startup-precomputed SQL identifier cache for all document types in `SchemaRegistry`.
#[derive(Debug, Clone)]
pub struct SchemaNaming {
    types: IndexMap<DocumentTypeId, DocumentTypeNaming>,
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl sea_query::Iden for StaticIden {
    fn unquoted(&self) -> &str {
        self.0
    }
}

impl From<&'static str> for StaticIden {
    fn from(s: &'static str) -> Self {
        Self(s)
    }
}

impl From<&SchemaRegistry> for SchemaNaming {
    fn from(registry: &SchemaRegistry) -> Self {
        Self::new(registry)
    }
}

// ---------------------------------------------------------------------------
// Method Implementations
// ---------------------------------------------------------------------------

impl DocumentTypeNaming {
    /// Returns the naming metadata for a specific schema field, if present.
    #[must_use]
    pub fn field_column(&self, attr_id: &AttributeId) -> Option<&FieldColumnNaming> {
        self.field_columns.get(attr_id)
    }

    /// Returns the naming metadata for a specific schema relation, if present.
    #[must_use]
    pub fn relation(&self, attr_id: &AttributeId) -> Option<&RelationNaming> {
        self.relations.get(attr_id)
    }
}

impl SchemaNaming {
    /// Builds the precomputed naming cache for all document types in `registry`.
    #[must_use]
    pub fn new(registry: &SchemaRegistry) -> Self {
        let table_names = collect_table_names(registry);
        let mut types = IndexMap::with_capacity(registry.iterate().count());

        for doc_type in registry.iterate() {
            let naming = build_document_type_naming(doc_type, registry, &table_names);
            types.insert(doc_type.id.clone(), naming);
        }

        Self { types }
    }

    /// Precomputes and leaks the naming cache as a `'static` reference for zero-allocation access.
    #[must_use]
    pub fn from_registry(registry: &SchemaRegistry) -> &'static Self {
        Box::leak(Box::new(Self::new(registry)))
    }

    /// Looks up naming metadata for a document type.
    #[must_use]
    pub fn get(&self, type_id: &DocumentTypeId) -> Option<&DocumentTypeNaming> {
        self.types.get(type_id)
    }

    /// Looks up naming metadata for a document type, returning an error if missing.
    ///
    /// # Errors
    /// Returns [`PersistenceError::Validation`] if the document type is unknown.
    pub fn require(
        &self,
        type_id: &DocumentTypeId,
    ) -> Result<&DocumentTypeNaming, PersistenceError> {
        self.get(type_id).ok_or_else(|| {
            PersistenceError::Validation(format!(
                "unknown document type '{type_id}' in SchemaNaming"
            ))
        })
    }

    /// Iterates over all registered document type naming entries.
    pub fn iter(&self) -> impl Iterator<Item = (&DocumentTypeId, &DocumentTypeNaming)> {
        self.types.iter()
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn collect_table_names(registry: &SchemaRegistry) -> IndexMap<DocumentTypeId, &'static str> {
    let mut names = IndexMap::with_capacity(registry.iterate().count());
    for doc_type in registry.iterate() {
        let table_name = leak_str(table_name_for_document_type(doc_type));
        names.insert(doc_type.id.clone(), table_name);
    }
    names
}

fn build_document_type_naming(
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
    table_names: &IndexMap<DocumentTypeId, &'static str>,
) -> DocumentTypeNaming {
    let table_name = match table_names.get(&doc_type.id) {
        Some(&name) => name,
        None => leak_str(table_name_for_document_type(doc_type)),
    };
    let table_iden = StaticIden(table_name);
    let field_columns = build_field_columns(doc_type);
    let wide_upsert_columns = build_wide_upsert_columns(&field_columns);
    let wide_update_columns = build_wide_update_columns(&field_columns);
    let relations = build_relations(doc_type, registry, table_names, table_name);

    DocumentTypeNaming {
        document_type_id: doc_type.id.clone(),
        table_name,
        table_iden,
        field_columns,
        wide_upsert_columns,
        wide_update_columns,
        relations,
    }
}

fn build_field_columns(doc_type: &DocumentType) -> IndexMap<AttributeId, FieldColumnNaming> {
    let mut columns = IndexMap::with_capacity(doc_type.fields.len());
    for field in &doc_type.fields {
        let col_name = leak_str(column_name(&field.id));
        columns.insert(
            field.id.clone(),
            FieldColumnNaming {
                column_name: col_name,
                column_iden: StaticIden(col_name),
                field_type: field.field_type,
            },
        );
    }
    columns
}

fn build_wide_upsert_columns(
    field_columns: &IndexMap<AttributeId, FieldColumnNaming>,
) -> &'static [StaticIden] {
    let mut cols = Vec::with_capacity(12 + field_columns.len());
    cols.push(StaticIden(COL_ID));
    cols.push(StaticIden(COL_HAS_PENDING_DRAFT));
    cols.push(StaticIden(COL_DISPLAY_VALUES));
    cols.push(StaticIden(COL_DISPLAY_TITLE));
    for f in field_columns.values() {
        cols.push(f.column_iden);
    }
    cols.push(StaticIden(COL_REVISION));
    cols.push(StaticIden(COL_PUBLISHED_AT));
    cols.push(StaticIden(COL_PUBLISHED_BY));
    cols.push(StaticIden(COL_CREATED_AT));
    cols.push(StaticIden(COL_CREATED_BY));
    cols.push(StaticIden(COL_UPDATED_AT));
    cols.push(StaticIden(COL_UPDATED_BY));
    Box::leak(cols.into_boxed_slice())
}

fn build_wide_update_columns(
    field_columns: &IndexMap<AttributeId, FieldColumnNaming>,
) -> &'static [StaticIden] {
    let mut cols = Vec::with_capacity(9 + field_columns.len());
    cols.push(StaticIden(COL_HAS_PENDING_DRAFT));
    cols.push(StaticIden(COL_DISPLAY_VALUES));
    cols.push(StaticIden(COL_DISPLAY_TITLE));
    for f in field_columns.values() {
        cols.push(f.column_iden);
    }
    cols.push(StaticIden(COL_REVISION));
    cols.push(StaticIden(COL_PUBLISHED_AT));
    cols.push(StaticIden(COL_PUBLISHED_BY));
    cols.push(StaticIden(COL_UPDATED_AT));
    cols.push(StaticIden(COL_UPDATED_BY));
    Box::leak(cols.into_boxed_slice())
}

fn build_relations(
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
    table_names: &IndexMap<DocumentTypeId, &'static str>,
    source_table_name: &'static str,
) -> IndexMap<AttributeId, RelationNaming> {
    let mut relations = IndexMap::with_capacity(doc_type.relations.len());
    for rel in &doc_type.relations {
        let target_table_name = match table_names.get(&rel.target) {
            Some(&tbl) => tbl,
            None => leak_str(kebab_to_snake(rel.target.as_ref())),
        };
        let target_table_iden = StaticIden(target_table_name);
        let junction_table_name = resolve_junction_table_name(
            rel,
            doc_type,
            registry,
            source_table_name,
            target_table_name,
        );
        let junction_table_iden = StaticIden(junction_table_name);

        relations.insert(
            rel.id.clone(),
            RelationNaming {
                attribute_id: rel.id.clone(),
                relation_type: rel.relation_type,
                target_type_id: rel.target.clone(),
                target_table_name,
                target_table_iden,
                junction_table_name,
                junction_table_iden,
            },
        );
    }
    relations
}

fn resolve_junction_table_name(
    rel: &RelationDefinition,
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
    source_table_name: &str,
    target_table_name: &str,
) -> &'static str {
    if rel.relation_type.is_owning() {
        leak_str(junction_table_name(source_table_name, &rel.id))
    } else {
        resolve_inverse_junction_table_name(
            rel,
            doc_type,
            registry,
            source_table_name,
            target_table_name,
        )
    }
}

fn resolve_inverse_junction_table_name(
    rel: &RelationDefinition,
    doc_type: &DocumentType,
    registry: &SchemaRegistry,
    source_table_name: &str,
    target_table_name: &str,
) -> &'static str {
    if let Some(target_doc_type) = registry.get(&rel.target) {
        let owning_rel = target_doc_type.relations.iter().find(|r| {
            r.target == doc_type.id && r.relation_type.is_owning()
        });
        if let Some(owner) = owning_rel {
            return leak_str(junction_table_name(target_table_name, &owner.id));
        }
    }
    leak_str(junction_table_name(source_table_name, &rel.id))
}

fn leak_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::common::DisplayName;
    use domain::schema::{
        DocumentKind, DocumentTypeInfo, DocumentTypeOptions, FieldDefinition, PrimitiveType,
    };
    use sea_query::Iden;
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
    fn test_static_iden_unquoted() {
        let iden = StaticIden("articles");
        assert_eq!(iden.unquoted(), "articles");
    }

    #[test]
    fn test_schema_naming_collection_and_singleton() {
        let coll_field = FieldDefinition {
            id: AttributeId::try_new("title").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            unique: false,
            required: true,
            constraints: HashSet::new(),
        };
        let coll = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![coll_field],
            vec![],
        );
        let single = make_test_doc_type(
            "site-settings",
            "site-settings-plural",
            DocumentKind::Singleton,
            vec![],
            vec![],
        );

        let registry = SchemaRegistry::new(vec![coll, single]);
        let naming = SchemaNaming::new(&registry);

        let article_naming = naming
            .get(&DocumentTypeId::try_new("article").unwrap())
            .unwrap();
        assert_eq!(article_naming.table_name, "articles");
        assert_eq!(article_naming.table_iden.unquoted(), "articles");

        let settings_naming = naming
            .get(&DocumentTypeId::try_new("site-settings").unwrap())
            .unwrap();
        assert_eq!(settings_naming.table_name, "site_settings");
        assert_eq!(settings_naming.table_iden.unquoted(), "site_settings");
    }

    #[test]
    fn test_schema_naming_column_ordering() {
        let title_field = FieldDefinition {
            id: AttributeId::try_new("title").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            unique: false,
            required: true,
            constraints: HashSet::new(),
        };
        let hero_image = FieldDefinition {
            id: AttributeId::try_new("hero-image-url").unwrap(),
            field_type: FieldType::Url,
            unique: false,
            required: false,
            constraints: HashSet::new(),
        };
        let doc_type = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![title_field, hero_image],
            vec![],
        );

        let registry = SchemaRegistry::new(vec![doc_type]);
        let naming = SchemaNaming::new(&registry);
        let dt = naming
            .require(&DocumentTypeId::try_new("article").unwrap())
            .unwrap();

        assert_eq!(dt.field_columns.len(), 2);
        let title_naming = dt
            .field_column(&AttributeId::try_new("title").unwrap())
            .unwrap();
        assert_eq!(title_naming.column_name, "title");
        assert_eq!(title_naming.column_iden.unquoted(), "title");

        let hero_naming = dt
            .field_column(&AttributeId::try_new("hero-image-url").unwrap())
            .unwrap();
        assert_eq!(hero_naming.column_name, "hero_image_url");
        assert_eq!(hero_naming.column_iden.unquoted(), "hero_image_url");

        let expected_upsert: Vec<&str> = vec![
            "id",
            "has_pending_draft",
            "display_values",
            "display_title",
            "title",
            "hero_image_url",
            "revision",
            "published_at",
            "published_by",
            "created_at",
            "created_by",
            "updated_at",
            "updated_by",
        ];
        let actual_upsert: Vec<&str> = dt
            .wide_upsert_columns
            .iter()
            .map(|i| i.unquoted())
            .collect();
        assert_eq!(actual_upsert, expected_upsert);

        let expected_update: Vec<&str> = vec![
            "has_pending_draft",
            "display_values",
            "display_title",
            "title",
            "hero_image_url",
            "revision",
            "published_at",
            "published_by",
            "updated_at",
            "updated_by",
        ];
        let actual_update: Vec<&str> = dt
            .wide_update_columns
            .iter()
            .map(|i| i.unquoted())
            .collect();
        assert_eq!(actual_update, expected_update);
    }

    #[test]
    fn test_schema_naming_owning_and_inverse_relation_junction_table() {
        let owning_rel = RelationDefinition {
            id: AttributeId::try_new("category").unwrap(),
            relation_type: RelationType::HasOne,
            target: DocumentTypeId::try_new("category").unwrap(),
        };
        let article = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![],
            vec![owning_rel],
        );

        let inverse_rel = RelationDefinition {
            id: AttributeId::try_new("articles").unwrap(),
            relation_type: RelationType::BelongsToMany,
            target: DocumentTypeId::try_new("article").unwrap(),
        };
        let category = make_test_doc_type(
            "category",
            "categories",
            DocumentKind::Collection,
            vec![],
            vec![inverse_rel],
        );

        let registry = SchemaRegistry::new(vec![article, category]);
        let naming = SchemaNaming::new(&registry);

        let article_naming = naming
            .get(&DocumentTypeId::try_new("article").unwrap())
            .unwrap();
        let cat_rel = article_naming
            .relation(&AttributeId::try_new("category").unwrap())
            .unwrap();
        assert_eq!(cat_rel.target_table_name, "categories");
        assert_eq!(cat_rel.junction_table_name, "articles_category_links");
        assert_eq!(
            cat_rel.junction_table_iden.unquoted(),
            "articles_category_links"
        );

        let category_naming = naming
            .get(&DocumentTypeId::try_new("category").unwrap())
            .unwrap();
        let art_rel = category_naming
            .relation(&AttributeId::try_new("articles").unwrap())
            .unwrap();
        assert_eq!(art_rel.target_table_name, "articles");
        // Inverse relation MUST resolve to the owning junction table: articles_category_links
        assert_eq!(art_rel.junction_table_name, "articles_category_links");
        assert_eq!(
            art_rel.junction_table_iden.unquoted(),
            "articles_category_links"
        );
    }

    #[test]
    fn test_schema_naming_require_missing_error() {
        let registry = SchemaRegistry::new(vec![]);
        let naming = SchemaNaming::new(&registry);
        let missing_id = DocumentTypeId::try_new("unknown").unwrap();
        let err = naming.require(&missing_id).unwrap_err();
        assert!(matches!(err, PersistenceError::Validation(_)));
        assert!(err.to_string().contains("unknown document type 'unknown'"));
    }

    #[test]
    fn test_schema_naming_iter_and_from_registry() {
        let t1 = make_test_doc_type("t1", "t1-plural", DocumentKind::Collection, vec![], vec![]);
        let t2 = make_test_doc_type("t2", "t2-plural", DocumentKind::Collection, vec![], vec![]);
        let registry = SchemaRegistry::new(vec![t1, t2]);
        let static_naming = SchemaNaming::from_registry(&registry);

        let ids: Vec<&str> = static_naming.iter().map(|(id, _)| id.as_ref()).collect();
        assert_eq!(ids, vec!["t1", "t2"]);
    }
}
