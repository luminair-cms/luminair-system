//! Shared Sea-Query helpers, StaticIden constants, pagination, filters, and value mapping.

use std::collections::HashMap;

use sea_query::{
    DeleteStatement, Expr, ExprTrait, InsertStatement, PostgresQueryBuilder, SelectStatement,
    UpdateStatement,
};
use sea_query_sqlx::{SqlxBinder, SqlxValues};

use domain::content::ports::{FieldFilter, Pagination};
use domain::content::values::{ContentValue, DomainValue, PrimitiveValue};
use domain::schema::{FieldType, IntegerSize, PrimitiveType};

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{
    ALIAS_COMBINED, COL_ACTION, COL_ASSIGNED_ROLES, COL_CONTENT, COL_CREATED_AT, COL_CREATED_BY,
    COL_DESCRIPTION, COL_DISPLAY_TITLE, COL_DISPLAY_VALUES, COL_DOCUMENT_ID,
    COL_DOCUMENT_TYPE_ID, COL_DRAFT_VERSION, COL_EMAIL, COL_FIRST_SEEN_AT, COL_GRANTED_AT,
    COL_GRANTED_BY, COL_HAS_PENDING_DRAFT, COL_ID, COL_LAST_PUBLISHED_REVISION, COL_LAST_SEEN_AT,
    COL_NAME, COL_ORDER_INDEX, COL_PUBLISHED_AT, COL_PUBLISHED_BY, COL_REASON,
    COL_REJECTION_REASON, COL_RELATIONS, COL_REQUESTED_AT, COL_REVIEWED_AT,
    COL_REVIEWED_BY, COL_REVISION, COL_ROLE_ID, COL_SCHEMA_VERSION, COL_SLUG, COL_SOURCE_ID,
    COL_STATUS, COL_TARGET_ID, COL_UPDATED_AT, COL_UPDATED_BY, COL_USER_ID,
    COL_VERSION, DocumentTypeNaming, StaticIden, TABLE_ACCESS_REQUESTS,
    TABLE_DOCUMENT_DRAFTS, TABLE_DOCUMENT_SNAPSHOTS, TABLE_DOCUMENT_TOMBSTONES,
    TABLE_ROLE_PERMISSIONS, TABLE_ROLES, TABLE_USER_PROFILES, TABLE_USER_ROLE_ASSIGNMENTS,
};
use crate::persistence::repositories::codec::{localized_text_to_json, primitive_value_to_json};

// ---------------------------------------------------------------------------
// Public StaticIden Constants
// ---------------------------------------------------------------------------

pub const COL_ID_IDEN: StaticIden = StaticIden(COL_ID);
pub const COL_REVISION_IDEN: StaticIden = StaticIden(COL_REVISION);
pub const COL_HAS_PENDING_DRAFT_IDEN: StaticIden = StaticIden(COL_HAS_PENDING_DRAFT);
pub const COL_DISPLAY_VALUES_IDEN: StaticIden = StaticIden(COL_DISPLAY_VALUES);
pub const COL_DISPLAY_TITLE_IDEN: StaticIden = StaticIden(COL_DISPLAY_TITLE);
pub const COL_CREATED_AT_IDEN: StaticIden = StaticIden(COL_CREATED_AT);
pub const COL_CREATED_BY_IDEN: StaticIden = StaticIden(COL_CREATED_BY);
pub const COL_UPDATED_AT_IDEN: StaticIden = StaticIden(COL_UPDATED_AT);
pub const COL_UPDATED_BY_IDEN: StaticIden = StaticIden(COL_UPDATED_BY);
pub const COL_PUBLISHED_AT_IDEN: StaticIden = StaticIden(COL_PUBLISHED_AT);
pub const COL_PUBLISHED_BY_IDEN: StaticIden = StaticIden(COL_PUBLISHED_BY);

pub const COL_STATUS_IDEN: StaticIden = StaticIden(COL_STATUS);
pub const COL_DOCUMENT_TYPE_ID_IDEN: StaticIden = StaticIden(COL_DOCUMENT_TYPE_ID);
pub const COL_CONTENT_IDEN: StaticIden = StaticIden(COL_CONTENT);
pub const COL_RELATIONS_IDEN: StaticIden = StaticIden(COL_RELATIONS);
pub const COL_SCHEMA_VERSION_IDEN: StaticIden = StaticIden(COL_SCHEMA_VERSION);
pub const COL_VERSION_IDEN: StaticIden = StaticIden(COL_VERSION);
pub const COL_LAST_PUBLISHED_REVISION_IDEN: StaticIden = StaticIden(COL_LAST_PUBLISHED_REVISION);
pub const COL_SLUG_IDEN: StaticIden = StaticIden(COL_SLUG);
pub const COL_DRAFT_VERSION_IDEN: StaticIden = StaticIden(COL_DRAFT_VERSION);
pub const ALIAS_COMBINED_IDEN: StaticIden = StaticIden(ALIAS_COMBINED);

pub const COL_REASON_IDEN: StaticIden = StaticIden(COL_REASON);
pub const COL_DOCUMENT_ID_IDEN: StaticIden = StaticIden(COL_DOCUMENT_ID);

pub const COL_SOURCE_ID_IDEN: StaticIden = StaticIden(COL_SOURCE_ID);
pub const COL_TARGET_ID_IDEN: StaticIden = StaticIden(COL_TARGET_ID);
pub const COL_ORDER_INDEX_IDEN: StaticIden = StaticIden(COL_ORDER_INDEX);

pub const COL_NAME_IDEN: StaticIden = StaticIden(COL_NAME);
pub const COL_DESCRIPTION_IDEN: StaticIden = StaticIden(COL_DESCRIPTION);
pub const COL_ROLE_ID_IDEN: StaticIden = StaticIden(COL_ROLE_ID);
pub const COL_ACTION_IDEN: StaticIden = StaticIden(COL_ACTION);
pub const COL_USER_ID_IDEN: StaticIden = StaticIden(COL_USER_ID);
pub const COL_GRANTED_AT_IDEN: StaticIden = StaticIden(COL_GRANTED_AT);
pub const COL_GRANTED_BY_IDEN: StaticIden = StaticIden(COL_GRANTED_BY);
pub const COL_EMAIL_IDEN: StaticIden = StaticIden(COL_EMAIL);
pub const COL_REQUESTED_AT_IDEN: StaticIden = StaticIden(COL_REQUESTED_AT);
pub const COL_REJECTION_REASON_IDEN: StaticIden = StaticIden(COL_REJECTION_REASON);
pub const COL_REVIEWED_BY_IDEN: StaticIden = StaticIden(COL_REVIEWED_BY);
pub const COL_REVIEWED_AT_IDEN: StaticIden = StaticIden(COL_REVIEWED_AT);
pub const COL_ASSIGNED_ROLES_IDEN: StaticIden = StaticIden(COL_ASSIGNED_ROLES);
pub const COL_FIRST_SEEN_AT_IDEN: StaticIden = StaticIden(COL_FIRST_SEEN_AT);
pub const COL_LAST_SEEN_AT_IDEN: StaticIden = StaticIden(COL_LAST_SEEN_AT);

pub const TABLE_ROLES_IDEN: StaticIden = StaticIden(TABLE_ROLES);
pub const TABLE_ROLE_PERMISSIONS_IDEN: StaticIden = StaticIden(TABLE_ROLE_PERMISSIONS);
pub const TABLE_USER_ROLE_ASSIGNMENTS_IDEN: StaticIden = StaticIden(TABLE_USER_ROLE_ASSIGNMENTS);
pub const TABLE_ACCESS_REQUESTS_IDEN: StaticIden = StaticIden(TABLE_ACCESS_REQUESTS);
pub const TABLE_USER_PROFILES_IDEN: StaticIden = StaticIden(TABLE_USER_PROFILES);
pub const TABLE_DOCUMENT_DRAFTS_IDEN: StaticIden = StaticIden(TABLE_DOCUMENT_DRAFTS);
pub const TABLE_DOCUMENT_SNAPSHOTS_IDEN: StaticIden = StaticIden(TABLE_DOCUMENT_SNAPSHOTS);
pub const TABLE_DOCUMENT_TOMBSTONES_IDEN: StaticIden = StaticIden(TABLE_DOCUMENT_TOMBSTONES);

// ---------------------------------------------------------------------------
// Public Compilation & Query Helpers
// ---------------------------------------------------------------------------

/// Compiles a Sea-Query `SelectStatement` to SQL and bound parameters for sqlx.
#[must_use]
pub fn compile_select(stmt: &SelectStatement) -> (String, SqlxValues) {
    stmt.build_sqlx(PostgresQueryBuilder)
}

/// Compiles a Sea-Query `InsertStatement` to SQL and bound parameters for sqlx.
#[must_use]
pub fn compile_insert(stmt: &InsertStatement) -> (String, SqlxValues) {
    stmt.build_sqlx(PostgresQueryBuilder)
}

/// Compiles a Sea-Query `UpdateStatement` to SQL and bound parameters for sqlx.
#[must_use]
pub fn compile_update(stmt: &UpdateStatement) -> (String, SqlxValues) {
    stmt.build_sqlx(PostgresQueryBuilder)
}

/// Compiles a Sea-Query `DeleteStatement` to SQL and bound parameters for sqlx.
#[must_use]
pub fn compile_delete(stmt: &DeleteStatement) -> (String, SqlxValues) {
    stmt.build_sqlx(PostgresQueryBuilder)
}

/// Applies pagination limit and offset to a `SelectStatement`.
pub fn apply_pagination(stmt: &mut SelectStatement, pagination: Pagination) {
    let page_offset = u64::from(pagination.page.saturating_sub(1));
    let page_size = u64::from(pagination.page_size);
    stmt.limit(page_size).offset(page_offset * page_size);
}

/// Applies equality filters on wide table field columns.
pub fn apply_wide_table_filters(
    stmt: &mut SelectStatement,
    type_naming: &DocumentTypeNaming,
    filters: &[FieldFilter],
) -> Result<(), PersistenceError> {
    for filter in filters {
        let col = type_naming.field_column(&filter.attribute_id).ok_or_else(|| {
            PersistenceError::Validation(format!(
                "unknown filter attribute '{}'",
                filter.attribute_id
            ))
        })?;
        let sea_val = typed_domain_value_to_sea_value(&col.field_type, &filter.value)?;
        stmt.and_where(Expr::col(col.column_iden).eq(sea_val));
    }
    Ok(())
}

/// Converts an untyped domain value into a generic Sea-Query SQL parameter value.
#[must_use]
pub fn domain_value_to_sea_value(val: &DomainValue) -> sea_query::Value {
    match val {
        DomainValue::Primitive(p) => primitive_value_to_sea_value(p),
        DomainValue::Email(e) => e.as_ref().to_string().into(),
        DomainValue::Url(u) => u.as_ref().to_string().into(),
        DomainValue::Json(j) => json_map_to_sea_value(j),
    }
}

/// Converts a domain value into a typed Sea-Query parameter, validating type compatibility and numeric bounds.
pub fn typed_domain_value_to_sea_value(
    field_type: &FieldType,
    val: &DomainValue,
) -> Result<sea_query::Value, PersistenceError> {
    if !val.matches_field_type(field_type) {
        return Err(PersistenceError::Validation(format!(
            "value does not match field type {field_type:?}"
        )));
    }

    match (field_type, val) {
        (
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)),
            DomainValue::Primitive(PrimitiveValue::Integer(i)),
        ) => {
            let v = i16::try_from(*i).map_err(|e| {
                PersistenceError::Validation(format!("integer {i} overflow for i16: {e}"))
            })?;
            Ok(sea_query::Value::SmallInt(Some(v)))
        }
        (
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
            DomainValue::Primitive(PrimitiveValue::Integer(i)),
        ) => {
            let v = i32::try_from(*i).map_err(|e| {
                PersistenceError::Validation(format!("integer {i} overflow for i32: {e}"))
            })?;
            Ok(sea_query::Value::Int(Some(v)))
        }
        (
            FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)),
            DomainValue::Primitive(PrimitiveValue::Integer(i)),
        ) => Ok(sea_query::Value::BigInt(Some(*i))),
        _ => Ok(domain_value_to_sea_value(val)),
    }
}

/// Maps an optional `ContentValue` to its corresponding Sea-Query value based on `FieldType`.
pub fn content_value_to_sea_value(
    field_type: &FieldType,
    val: Option<&ContentValue>,
) -> Result<sea_query::Value, PersistenceError> {
    match val {
        None | Some(ContentValue::Null) => Ok(typed_null_sea_value(field_type)),
        Some(ContentValue::Scalar(dv)) => typed_domain_value_to_sea_value(field_type, dv),
        Some(ContentValue::LocalizedText(map)) => {
            if *field_type != FieldType::LocalizedText {
                return Err(PersistenceError::Validation(format!(
                    "expected {field_type:?}, found LocalizedText"
                )));
            }
            let json_val = localized_text_to_json(map);
            Ok(sea_query::Value::Json(Some(Box::new(json_val))))
        }
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn typed_null_sea_value(field_type: &FieldType) -> sea_query::Value {
    match field_type {
        FieldType::Primitive(PrimitiveType::Uid | PrimitiveType::Text)
        | FieldType::Email
        | FieldType::Url => sea_query::Value::String(None),
        FieldType::Primitive(PrimitiveType::Uuid) => sea_query::Value::Uuid(None),
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16)) => {
            sea_query::Value::SmallInt(None)
        }
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)) => {
            sea_query::Value::Int(None)
        }
        FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64)) => {
            sea_query::Value::BigInt(None)
        }
        FieldType::Primitive(PrimitiveType::Decimal { .. }) => sea_query::Value::Decimal(None),
        FieldType::Primitive(PrimitiveType::Date) => sea_query::Value::ChronoDate(None),
        FieldType::Primitive(PrimitiveType::DateTime) => {
            sea_query::Value::ChronoDateTimeUtc(None)
        }
        FieldType::Primitive(PrimitiveType::Boolean) => sea_query::Value::Bool(None),
        FieldType::Json | FieldType::LocalizedText => sea_query::Value::Json(None),
    }
}

fn primitive_value_to_sea_value(p: &PrimitiveValue) -> sea_query::Value {
    match p {
        PrimitiveValue::Text(s) | PrimitiveValue::Uid(s) => s.clone().into(),
        PrimitiveValue::Uuid(u) => (*u).into(),
        PrimitiveValue::Integer(i) => (*i).into(),
        PrimitiveValue::Decimal(d) => (*d).into(),
        PrimitiveValue::Date(d) => (*d).into(),
        PrimitiveValue::DateTime(dt) => (*dt).into(),
        PrimitiveValue::Boolean(b) => (*b).into(),
    }
}

fn json_map_to_sea_value(map: &HashMap<String, PrimitiveValue>) -> sea_query::Value {
    let json_map: serde_json::Map<String, serde_json::Value> = map
        .iter()
        .map(|(k, v)| (k.clone(), primitive_value_to_json(v)))
        .collect();
    serde_json::Value::Object(json_map).into()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use domain::common::DisplayName;
    use domain::schema::{
        AttributeId, DocumentKind, DocumentType, DocumentTypeId, DocumentTypeInfo,
        DocumentTypeOptions, FieldDefinition, SchemaRegistry,
    };
    use domain::system::LocaleId;
    use sea_query::Query;

    fn make_test_doc_type(
        id: &str,
        plural: &str,
        kind: DocumentKind,
        fields: Vec<FieldDefinition>,
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
            relations: Default::default(),
        }
    }

    #[test]
    fn test_apply_pagination() {
        let mut stmt = Query::select();
        stmt.from(COL_ID_IDEN);
        apply_pagination(&mut stmt, Pagination { page: 3, page_size: 15 });
        let (sql, _) = compile_select(&stmt);
        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("LIMIT 15"));
        assert!(unparam.contains("OFFSET 30"));
        assert!(sql.contains("LIMIT $1"));
        assert!(sql.contains("OFFSET $2"));
    }

    #[test]
    fn test_content_value_to_sea_value_nulls() {
        let text_ft = FieldType::Primitive(PrimitiveType::Text);
        assert_eq!(content_value_to_sea_value(&text_ft, None).unwrap(), sea_query::Value::String(None));
        assert_eq!(
            content_value_to_sea_value(&text_ft, Some(&ContentValue::Null)).unwrap(),
            sea_query::Value::String(None)
        );

        let i16_ft = FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16));
        assert_eq!(content_value_to_sea_value(&i16_ft, None).unwrap(), sea_query::Value::SmallInt(None));

        let i32_ft = FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32));
        assert_eq!(content_value_to_sea_value(&i32_ft, None).unwrap(), sea_query::Value::Int(None));

        let i64_ft = FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64));
        assert_eq!(content_value_to_sea_value(&i64_ft, None).unwrap(), sea_query::Value::BigInt(None));

        let loc_ft = FieldType::LocalizedText;
        assert_eq!(content_value_to_sea_value(&loc_ft, None).unwrap(), sea_query::Value::Json(None));
    }

    #[test]
    fn test_content_value_to_sea_value_integers() {
        let i16_ft = FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16));
        let val_16 = ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(100)));
        assert_eq!(
            content_value_to_sea_value(&i16_ft, Some(&val_16)).unwrap(),
            sea_query::Value::SmallInt(Some(100))
        );

        let overflow_16 = ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(70000)));
        assert!(content_value_to_sea_value(&i16_ft, Some(&overflow_16)).is_err());

        let i32_ft = FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32));
        let val_32 = ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(100_000)));
        assert_eq!(
            content_value_to_sea_value(&i32_ft, Some(&val_32)).unwrap(),
            sea_query::Value::Int(Some(100_000))
        );

        let i64_ft = FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64));
        let val_64 = ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(10_000_000_000)));
        assert_eq!(
            content_value_to_sea_value(&i64_ft, Some(&val_64)).unwrap(),
            sea_query::Value::BigInt(Some(10_000_000_000))
        );
    }

    #[test]
    fn test_content_value_to_sea_value_localized_text() {
        let loc_ft = FieldType::LocalizedText;
        let mut map = HashMap::new();
        map.insert(LocaleId::try_new("en-US").unwrap(), "Hello".to_string());
        map.insert(LocaleId::try_new("fr-FR").unwrap(), "Bonjour".to_string());
        let val = ContentValue::LocalizedText(map);

        let sea_val = content_value_to_sea_value(&loc_ft, Some(&val)).unwrap();
        match sea_val {
            sea_query::Value::Json(Some(json_box)) => {
                let obj = json_box.as_object().unwrap();
                assert_eq!(obj.get("en-US").unwrap(), "Hello");
                assert_eq!(obj.get("fr-FR").unwrap(), "Bonjour");
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    #[test]
    fn test_apply_wide_table_filters() {
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
        let doc_type = make_test_doc_type(
            "article",
            "articles",
            DocumentKind::Collection,
            vec![title_field, views_field],
        );

        let registry = SchemaRegistry::new(vec![doc_type]);
        let schema_naming = crate::persistence::naming::SchemaNaming::new(&registry);
        let type_naming = schema_naming
            .get(&DocumentTypeId::try_new("article").unwrap())
            .unwrap();

        let mut stmt = Query::select();
        stmt.from(type_naming.table_iden);

        let filters = vec![
            FieldFilter {
                attribute_id: AttributeId::try_new("title").unwrap(),
                value: DomainValue::Primitive(PrimitiveValue::Text("Rust News".to_string())),
            },
            FieldFilter {
                attribute_id: AttributeId::try_new("views").unwrap(),
                value: DomainValue::Primitive(PrimitiveValue::Integer(500)),
            },
        ];

        apply_wide_table_filters(&mut stmt, type_naming, &filters).unwrap();
        let unparam = stmt.to_string(PostgresQueryBuilder);
        assert!(unparam.contains("\"title\" = 'Rust News'"));
        assert!(unparam.contains("\"views\" = 500"));
        let (sql, _) = compile_select(&stmt);
        assert!(sql.contains("\"title\" = $1"));
        assert!(sql.contains("\"views\" = $2"));

        let invalid_filter = vec![FieldFilter {
            attribute_id: AttributeId::try_new("nonexistent").unwrap(),
            value: DomainValue::Primitive(PrimitiveValue::Text("val".to_string())),
        }];
        assert!(apply_wide_table_filters(&mut stmt, type_naming, &invalid_filter).is_err());
    }
}
