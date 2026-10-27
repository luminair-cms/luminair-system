//! SQL database identifier generation, table naming conventions, and column mapping.

use domain::schema::{
    AttributeId, DocumentKind, DocumentType, DocumentTypeId, FieldType,
};

use super::sanitize::{join_snake_segments, kebab_to_snake};

// ---------------------------------------------------------------------------
// Standard Column Constants
// ---------------------------------------------------------------------------

pub const COL_ID: &str = "id";
pub const COL_REVISION: &str = "revision";
pub const COL_HAS_PENDING_DRAFT: &str = "has_pending_draft";
pub const COL_DISPLAY_VALUES: &str = "display_values";
pub const COL_DISPLAY_TITLE: &str = "display_title";
pub const COL_CREATED_AT: &str = "created_at";
pub const COL_CREATED_BY: &str = "created_by";
pub const COL_UPDATED_AT: &str = "updated_at";
pub const COL_UPDATED_BY: &str = "updated_by";
pub const COL_PUBLISHED_AT: &str = "published_at";
pub const COL_PUBLISHED_BY: &str = "published_by";

// Drafts table specific columns
pub const COL_STATUS: &str = "status";
pub const COL_DOCUMENT_TYPE_ID: &str = "document_type_id";
pub const COL_CONTENT: &str = "content";
pub const COL_RELATIONS: &str = "relations";
pub const COL_SCHEMA_VERSION: &str = "schema_version";
pub const COL_VERSION: &str = "version";
pub const COL_LAST_PUBLISHED_REVISION: &str = "last_published_revision";

// Snapshots table specific columns
pub const COL_REASON: &str = "reason";
pub const COL_DOCUMENT_ID: &str = "document_id";

// Junction table specific columns
pub const COL_SOURCE_ID: &str = "source_id";
pub const COL_TARGET_ID: &str = "target_id";
pub const COL_ORDER_INDEX: &str = "order_index";

// Auth / RBAC & Access Request specific columns
pub const COL_NAME: &str = "name";
pub const COL_DESCRIPTION: &str = "description";
pub const COL_ROLE_ID: &str = "role_id";
pub const COL_ACTION: &str = "action";
pub const COL_USER_ID: &str = "user_id";
pub const COL_GRANTED_AT: &str = "granted_at";
pub const COL_GRANTED_BY: &str = "granted_by";
pub const COL_EMAIL: &str = "email";
pub const COL_REQUESTED_AT: &str = "requested_at";
pub const COL_REJECTION_REASON: &str = "rejection_reason";
pub const COL_REVIEWED_BY: &str = "reviewed_by";
pub const COL_REVIEWED_AT: &str = "reviewed_at";
pub const COL_ASSIGNED_ROLES: &str = "assigned_roles";

// Static system table names
pub const TABLE_ROLES: &str = "roles";
pub const TABLE_ROLE_PERMISSIONS: &str = "role_permissions";
pub const TABLE_USER_ROLE_ASSIGNMENTS: &str = "user_role_assignments";
pub const TABLE_ACCESS_REQUESTS: &str = "access_requests";
pub const TABLE_SHADOW_USERS: &str = "shadow_users";
pub const TABLE_DOCUMENT_DRAFTS: &str = "document_drafts";
pub const TABLE_DOCUMENT_SNAPSHOTS: &str = "document_snapshots";
pub const TABLE_DOCUMENT_TOMBSTONES: &str = "document_tombstones";

// ---------------------------------------------------------------------------
// Public Identifier Generation Functions
// ---------------------------------------------------------------------------

/// Computes the SQL wide table name for a given `DocumentType`.
/// - Singletons use their singular name in `snake_case`.
/// - Collections use their plural name in `snake_case`.
#[must_use]
pub fn table_name_for_document_type(doc_type: &DocumentType) -> String {
    match doc_type.kind {
        DocumentKind::Singleton => kebab_to_snake(doc_type.info.singular_name.as_ref()),
        DocumentKind::Collection => kebab_to_snake(doc_type.info.plural_name.as_ref()),
    }
}

/// Computes the SQL wide table name from identifier components.
#[must_use]
pub fn table_name_from_id(
    type_id: &DocumentTypeId,
    kind: DocumentKind,
    plural_name: Option<&DocumentTypeId>,
) -> String {
    match (kind, plural_name) {
        (DocumentKind::Collection, Some(plural)) => kebab_to_snake(plural.as_ref()),
        _ => kebab_to_snake(type_id.as_ref()),
    }
}

/// Computes the relational junction link table name: `[source_table]_[rel_attr]_links`.
#[must_use]
pub fn junction_table_name(source_table: &str, rel_attr: &AttributeId) -> String {
    join_snake_segments(&[source_table, rel_attr.as_ref(), "links"])
}

/// Maps an `AttributeId` to its SQL column identifier (kebab-case to `snake_case`).
#[must_use]
pub fn column_name(attr_id: &AttributeId) -> String {
    join_snake_segments(&[attr_id.as_ref()])
}

/// Computes a standard index or unique index identifier: `idx_[table]_[col]` or `uq_[table]_[col]`.
#[must_use]
pub fn index_name(table_name: &str, col_name: &str, unique: bool) -> String {
    let prefix = if unique { "uq" } else { "idx" };
    join_snake_segments(&[prefix, table_name, col_name])
}

/// Computes a foreign key constraint identifier: `fk_[junction_table]_[side]`.
#[must_use]
pub fn foreign_key_name(junction_table: &str, side: &str) -> String {
    join_snake_segments(&["fk", junction_table, side])
}

/// Maps a domain `FieldType` to its physical SQL DDL type definition.
#[must_use]
pub fn sql_type_for_field(field_type: &FieldType) -> String {
    crate::persistence::migration::SqlColumnType::from(*field_type).to_sql_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::common::DisplayName;
    use domain::schema::{DocumentTypeInfo, DocumentTypeOptions, IntegerSize, PrimitiveType};
    use indexmap::IndexSet;

    fn make_test_doc_type(id: &str, plural: &str, kind: DocumentKind) -> DocumentType {
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
            fields: IndexSet::new(),
            relations: IndexSet::new(),
        }
    }

    #[test]
    fn test_table_name_collection_uses_plural() {
        let doc_type = make_test_doc_type("article", "articles", DocumentKind::Collection);
        assert_eq!(table_name_for_document_type(&doc_type), "articles");

        let blog_post = make_test_doc_type("blog-post", "blog-posts", DocumentKind::Collection);
        assert_eq!(table_name_for_document_type(&blog_post), "blog_posts");
    }

    #[test]
    fn test_table_name_singleton_uses_singular() {
        let doc_type = make_test_doc_type(
            "site-settings",
            "site-settings-plural",
            DocumentKind::Singleton,
        );
        assert_eq!(table_name_for_document_type(&doc_type), "site_settings");
    }

    #[test]
    fn test_junction_table_name() {
        let attr = AttributeId::try_new("author-tags").unwrap();
        assert_eq!(
            junction_table_name("articles", &attr),
            "articles_author_tags_links"
        );
    }

    #[test]
    fn test_column_name() {
        let attr = AttributeId::try_new("hero-image-url").unwrap();
        assert_eq!(column_name(&attr), "hero_image_url");
    }

    #[test]
    fn test_index_name() {
        assert_eq!(
            index_name("articles", "published_at", false),
            "idx_articles_published_at"
        );
        assert_eq!(
            index_name("articles", "slug", true),
            "uq_articles_slug"
        );
        assert_eq!(
            index_name("blog-posts", "hero-image", false),
            "idx_blog_posts_hero_image"
        );
    }

    #[test]
    fn test_foreign_key_name() {
        assert_eq!(
            foreign_key_name("articles_tags_links", "source"),
            "fk_articles_tags_links_source"
        );
        assert_eq!(
            foreign_key_name("articles_tags_links", "target"),
            "fk_articles_tags_links_target"
        );
    }

    #[test]
    fn test_sql_type_for_field() {
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Uid)),
            "VARCHAR(255)"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Uuid)),
            "UUID"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Text)),
            "TEXT"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I16
            ))),
            "SMALLINT"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I32
            ))),
            "INTEGER"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Integer(
                IntegerSize::I64
            ))),
            "BIGINT"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Decimal {
                precision: 10,
                scale: 2
            })),
            "NUMERIC(10, 2)"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Date)),
            "DATE"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::DateTime)),
            "TIMESTAMPTZ"
        );
        assert_eq!(
            sql_type_for_field(&FieldType::Primitive(PrimitiveType::Boolean)),
            "BOOLEAN"
        );
        assert_eq!(sql_type_for_field(&FieldType::Email), "VARCHAR(320)");
        assert_eq!(sql_type_for_field(&FieldType::Url), "TEXT");
        assert_eq!(sql_type_for_field(&FieldType::Json), "JSONB");
        assert_eq!(sql_type_for_field(&FieldType::LocalizedText), "JSONB");
    }
}
