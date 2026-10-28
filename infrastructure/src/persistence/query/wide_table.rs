//! Sea-Query dynamic query builders for document wide tables.
//!
//! Provides SQL statement construction for published wide table operations:
//! single-instance reads, visitor pagination queries, filters, counts, upserts,
//! and draft flag updates.

use chrono::{DateTime, Utc};
use sea_query::{
    Asterisk, DeleteStatement, Expr, ExprTrait, Func, InsertStatement, OnConflict, Order, Query,
    SelectStatement, SimpleExpr, UpdateStatement,
};
use uuid::Uuid;

use domain::content::instance::{DocumentInstance, PublicationState};
use domain::content::ports::{FieldFilter, Pagination};

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::DocumentTypeNaming;
use crate::persistence::query::common::{
    apply_pagination, apply_wide_table_filters, content_value_to_sea_value,
    COL_HAS_PENDING_DRAFT_IDEN, COL_ID_IDEN, COL_PUBLISHED_AT_IDEN,
};

// ---------------------------------------------------------------------------
// Public Wide Table Query Builders
// ---------------------------------------------------------------------------

/// Builds a `SELECT * FROM [table] WHERE id = $1` query statement.
#[must_use]
pub fn select_by_id_query(type_naming: &DocumentTypeNaming, id: Uuid) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(type_naming.table_iden)
        .column(Asterisk)
        .and_where(Expr::col(COL_ID_IDEN).eq(id));
    stmt
}

/// Builds a `SELECT * FROM [table] WHERE has_pending_draft = false` query statement.
#[must_use]
pub fn select_clean_published_query(type_naming: &DocumentTypeNaming) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(type_naming.table_iden)
        .column(Asterisk)
        .and_where(Expr::col(COL_HAS_PENDING_DRAFT_IDEN).eq(false));
    stmt
}

/// Builds a paginated `find_published` query with field equality filters.
///
/// Per ADR-004, all rows in `[type]` are published documents (modified rows retain
/// live published data here while draft edits live in `document_drafts`), so
/// `has_pending_draft` is NOT filtered.
pub fn find_published_query(
    type_naming: &DocumentTypeNaming,
    filters: &[FieldFilter],
    pagination: Pagination,
) -> Result<SelectStatement, PersistenceError> {
    let mut stmt = Query::select();
    stmt.from(type_naming.table_iden).column(Asterisk);

    apply_wide_table_filters(&mut stmt, type_naming, filters)?;

    stmt.order_by(COL_PUBLISHED_AT_IDEN, Order::Desc)
        .order_by(COL_ID_IDEN, Order::Asc);

    apply_pagination(&mut stmt, pagination);

    Ok(stmt)
}

/// Builds a `COUNT(id)` query for published rows matching the given filters.
pub fn count_published_query(
    type_naming: &DocumentTypeNaming,
    filters: &[FieldFilter],
) -> Result<SelectStatement, PersistenceError> {
    let mut stmt = Query::select();
    stmt.from(type_naming.table_iden)
        .expr(Func::count(Expr::col(COL_ID_IDEN)));

    apply_wide_table_filters(&mut stmt, type_naming, filters)?;

    Ok(stmt)
}

/// Builds an upsert statement for publishing a document instance into its wide table.
pub fn upsert_wide_table_query(
    type_naming: &DocumentTypeNaming,
    instance: &DocumentInstance,
    revision: u32,
    display_values_json: serde_json::Value,
    display_title: &str,
) -> Result<InsertStatement, PersistenceError> {
    let (published_at, published_by) = extract_published_audit(instance);
    let rev_i32 = i32::try_from(revision).unwrap_or(i32::MAX);

    let mut values: Vec<SimpleExpr> = Vec::with_capacity(type_naming.wide_upsert_columns.len());
    values.push(instance.id.into_inner().into());
    values.push(false.into());
    values.push(display_values_json.into());
    values.push(display_title.to_string().into());

    for (attr_id, col_naming) in &type_naming.field_columns {
        let sea_val = content_value_to_sea_value(
            &col_naming.field_type,
            instance.content.get_field(attr_id),
        )?;
        values.push(sea_val.into());
    }

    values.push(rev_i32.into());
    values.push(published_at.into());
    values.push(published_by.into());
    values.push(instance.audit.created_at.into());
    values.push(instance.audit.created_by.to_urn().into());
    values.push(instance.audit.updated_at.into());
    values.push(instance.audit.updated_by.to_urn().into());

    let mut stmt = Query::insert();
    stmt.into_table(type_naming.table_iden)
        .columns(type_naming.wide_upsert_columns.iter().copied())
        .values(values)
        .map_err(|e| PersistenceError::Validation(e.to_string()))?
        .on_conflict(
            OnConflict::column(COL_ID_IDEN)
                .update_columns(type_naming.wide_update_columns.iter().copied())
                .to_owned(),
        );

    Ok(stmt)
}

/// Builds an `UPDATE [table] SET has_pending_draft = $1 WHERE id = $2` query statement.
#[must_use]
pub fn update_has_pending_draft_query(
    type_naming: &DocumentTypeNaming,
    id: Uuid,
    has_pending_draft: bool,
) -> UpdateStatement {
    let mut stmt = Query::update();
    stmt.table(type_naming.table_iden)
        .value(COL_HAS_PENDING_DRAFT_IDEN, has_pending_draft)
        .and_where(Expr::col(COL_ID_IDEN).eq(id));
    stmt
}

/// Builds a `DELETE FROM [table] WHERE id = $1` query statement.
#[must_use]
pub fn delete_by_id_query(type_naming: &DocumentTypeNaming, id: Uuid) -> DeleteStatement {
    let mut stmt = Query::delete();
    stmt.from_table(type_naming.table_iden)
        .and_where(Expr::col(COL_ID_IDEN).eq(id));
    stmt
}

/// Builds a `SELECT 1 FROM [table] LIMIT 1` query statement to check table existence/emptiness.
#[must_use]
pub fn exists_in_wide_table_query(type_naming: &DocumentTypeNaming) -> SelectStatement {
    let mut stmt = Query::select();
    stmt.from(type_naming.table_iden)
        .expr(Expr::val(1))
        .limit(1);
    stmt
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn extract_published_audit(instance: &DocumentInstance) -> (DateTime<Utc>, String) {
    match &instance.content.publication_state {
        PublicationState::Published {
            published_at,
            published_by,
            ..
        }
        | PublicationState::Modified {
            published_at,
            published_by,
            ..
        } => (*published_at, published_by.to_urn()),
        PublicationState::Draft | PublicationState::Unpublished { .. } => {
            (instance.audit.updated_at, instance.audit.updated_by.to_urn())
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use chrono::TimeZone;
    use domain::auth::ActorId;
    use domain::common::DisplayName;
    use domain::content::instance::{
        AuditTrail, DocumentContent, DocumentInstanceId, PublicationState,
    };
    use domain::content::values::{ContentValue, DomainValue, PrimitiveValue};
    use domain::schema::{
        AttributeId, DocumentKind, DocumentType, DocumentTypeId, DocumentTypeInfo,
        DocumentTypeOptions, FieldDefinition, FieldType, IntegerSize, PrimitiveType,
        SchemaRegistry,
    };
    use sea_query::PostgresQueryBuilder;

    fn make_test_type_naming() -> DocumentTypeNaming {
        let title_field = FieldDefinition {
            id: AttributeId::try_new("title").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Text),
            unique: false,
            required: true,
            constraints: HashSet::new(),
        };
        let views_field = FieldDefinition {
            id: AttributeId::try_new("views").unwrap(),
            field_type: FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            unique: false,
            required: false,
            constraints: HashSet::new(),
        };
        let type_id = DocumentTypeId::try_new("article").unwrap();
        let plural_id = DocumentTypeId::try_new("articles").unwrap();
        let doc_type = DocumentType {
            id: type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Article").unwrap(),
                singular_name: type_id,
                plural_name: plural_id,
                description: None,
                display_fields: vec![],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: [title_field, views_field].into_iter().collect(),
            relations: Default::default(),
        };

        let registry = SchemaRegistry::new(vec![doc_type]);
        let schema_naming = crate::persistence::naming::SchemaNaming::new(&registry);
        schema_naming
            .get(&DocumentTypeId::try_new("article").unwrap())
            .unwrap()
            .clone()
    }

    #[test]
    fn test_select_by_id_query() {
        let type_naming = make_test_type_naming();
        let id = Uuid::nil();
        let stmt = select_by_id_query(&type_naming, id);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("SELECT * FROM \"articles\" WHERE \"id\" ="));
    }

    #[test]
    fn test_find_published_query() {
        let type_naming = make_test_type_naming();
        let filter = FieldFilter {
            attribute_id: AttributeId::try_new("title").unwrap(),
            value: DomainValue::Primitive(PrimitiveValue::Text("Rust News".to_string())),
        };
        let pagination = Pagination {
            page: 2,
            page_size: 10,
        };
        let stmt = find_published_query(&type_naming, &[filter], pagination).unwrap();
        let sql = stmt.to_string(PostgresQueryBuilder);

        assert!(sql.contains("SELECT * FROM \"articles\""));
        assert!(!sql.contains("\"has_pending_draft\" = FALSE"));
        assert!(sql.contains("\"title\" = 'Rust News'"));
        assert!(sql.contains("ORDER BY \"published_at\" DESC, \"id\" ASC"));
        assert!(sql.contains("LIMIT 10 OFFSET 10"));
    }

    #[test]
    fn test_count_published_query() {
        let type_naming = make_test_type_naming();
        let stmt = count_published_query(&type_naming, &[]).unwrap();
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("SELECT COUNT(\"id\") FROM \"articles\""));
        assert!(!sql.contains("\"has_pending_draft\""));
    }

    #[test]
    fn test_upsert_wide_table_query() {
        let type_naming = make_test_type_naming();
        let id = Uuid::nil();
        let fixed_time = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let user = ActorId::User(domain::auth::UserId::try_new("alice").unwrap());

        let mut fields = HashMap::new();
        fields.insert(
            AttributeId::try_new("title").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "My Post".to_string(),
            ))),
        );
        fields.insert(
            AttributeId::try_new("views").unwrap(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(42))),
        );

        let instance = DocumentInstance {
            id: DocumentInstanceId::new(id),
            document_type_id: DocumentTypeId::try_new("article").unwrap(),
            content: DocumentContent::new(
                fields,
                PublicationState::Published {
                    revision: 1,
                    published_at: fixed_time,
                    published_by: user.clone(),
                },
            ),
            relations: HashMap::new(),
            populated_relations: HashMap::new(),
            audit: AuditTrail {
                created_at: fixed_time,
                created_by: user.clone(),
                updated_at: fixed_time,
                updated_by: user,
                version: 1,
            },
        };

        let stmt = upsert_wide_table_query(
            &type_naming,
            &instance,
            1,
            serde_json::json!({"title": "My Post"}),
            "My Post",
        )
        .unwrap();

        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.starts_with("INSERT INTO \"articles\" (\"id\", \"has_pending_draft\", \"display_values\", \"display_title\", \"title\", \"views\", \"revision\", \"published_at\", \"published_by\", \"created_at\", \"created_by\", \"updated_at\", \"updated_by\")"));
        assert!(sql.contains("ON CONFLICT (\"id\") DO UPDATE SET"));
        assert!(sql.contains("\"title\" = \"excluded\".\"title\""));
        assert!(sql.contains("\"views\" = \"excluded\".\"views\""));
    }

    #[test]
    fn test_update_has_pending_draft_query() {
        let type_naming = make_test_type_naming();
        let id = Uuid::nil();
        let stmt = update_has_pending_draft_query(&type_naming, id, true);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("UPDATE \"articles\" SET \"has_pending_draft\" = TRUE WHERE \"id\" ="));
    }

    #[test]
    fn test_delete_by_id_query() {
        let type_naming = make_test_type_naming();
        let id = Uuid::nil();
        let stmt = delete_by_id_query(&type_naming, id);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("DELETE FROM \"articles\" WHERE \"id\" ="));
    }

    #[test]
    fn test_exists_in_wide_table_query() {
        let type_naming = make_test_type_naming();
        let stmt = exists_in_wide_table_query(&type_naming);
        let sql = stmt.to_string(PostgresQueryBuilder);
        assert!(sql.contains("SELECT 1 FROM \"articles\" LIMIT 1"));
    }
}
