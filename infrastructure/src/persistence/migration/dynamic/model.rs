//! Database Schema AST modeled with `IndexSet<T>` and `Borrow<str>`.
//!
//! Preserves deterministic table, column, and index order without duplicating
//! identifier strings for map keys. Enables O(1) lookups via `.get("name")`.

use std::borrow::Borrow;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

use indexmap::IndexSet;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Schema AST Types
// ---------------------------------------------------------------------------

/// The architectural classification of a database table in Luminair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(super) enum TableKind {
    /// A main document instance wide relational table (e.g. `articles`, `site_settings`).
    #[default]
    Entity,
    /// A relational link junction table (e.g. `articles_tags_links`, `articles_category_links`).
    Link,
}

/// Referential action triggered on update or delete of a foreign key constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub(super) enum ForeignKeyAction {
    #[default]
    Cascade,
    Restrict,
    SetNull,
    NoAction,
}

/// SQL column data types supported by Luminair and AWS Aurora DSQL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum SqlColumnType {
    Uuid,
    Varchar(Option<usize>),
    Text,
    SmallInt,
    Integer,
    BigInt,
    Decimal(u8, u8),
    Boolean,
    Date,
    Timestamptz,
    Jsonb,
}

/// Definition of a database column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ColumnDefinition {
    pub(super) name: String,
    pub(super) data_type: SqlColumnType,
    pub(super) nullable: bool,
    pub(super) is_primary_key: bool,
    pub(super) default_value: Option<String>,
}

/// Definition of a database index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct IndexDefinition {
    pub(super) name: String,
    pub(super) table_name: String,
    pub(super) columns: Vec<String>,
    pub(super) unique: bool,
}

/// Definition of a database foreign key constraint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ForeignKeyDefinition {
    pub(super) name: String,
    pub(super) columns: Vec<String>,
    pub(super) referenced_table: String,
    pub(super) referenced_columns: Vec<String>,
    pub(super) on_delete: ForeignKeyAction,
    pub(super) on_update: ForeignKeyAction,
}

/// Definition of a database table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TableDefinition {
    pub(super) name: String,
    pub(super) columns: IndexSet<ColumnDefinition>,
    pub(super) indexes: IndexSet<IndexDefinition>,
    pub(super) foreign_keys: IndexSet<ForeignKeyDefinition>,
    pub(super) kind: TableKind,
}

/// High-level representation of a relational database schema.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(super) struct DatabaseSchema {
    pub(super) tables: IndexSet<TableDefinition>,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl SqlColumnType {
    /// Formats the column type into standard PostgreSQL DDL syntax.
    #[must_use]
    pub(crate) fn to_sql_string(&self) -> String {
        match self {
            SqlColumnType::Uuid => "UUID".to_string(),
            SqlColumnType::Varchar(Some(len)) => format!("VARCHAR({len})"),
            SqlColumnType::Varchar(None) => "VARCHAR".to_string(),
            SqlColumnType::Text => "TEXT".to_string(),
            SqlColumnType::SmallInt => "SMALLINT".to_string(),
            SqlColumnType::Integer => "INTEGER".to_string(),
            SqlColumnType::BigInt => "BIGINT".to_string(),
            SqlColumnType::Decimal(p, s) => format!("NUMERIC({p}, {s})"),
            SqlColumnType::Boolean => "BOOLEAN".to_string(),
            SqlColumnType::Date => "DATE".to_string(),
            SqlColumnType::Timestamptz => "TIMESTAMPTZ".to_string(),
            SqlColumnType::Jsonb => "JSONB".to_string(),
        }
    }

    /// Parses a data type name from PostgreSQL `information_schema.columns`.
    #[must_use]
    pub(crate) fn from_information_schema(
        data_type: &str,
        char_len: Option<i32>,
        numeric_precision: Option<i32>,
        numeric_scale: Option<i32>,
    ) -> Option<Self> {
        match data_type.to_ascii_lowercase().as_str() {
            "uuid" => Some(SqlColumnType::Uuid),
            "character varying" | "varchar" => {
                let len = char_len.and_then(|l| usize::try_from(l).ok().filter(|&x| x > 0));
                Some(SqlColumnType::Varchar(len))
            }
            "text" => Some(SqlColumnType::Text),
            "smallint" => Some(SqlColumnType::SmallInt),
            "integer" => Some(SqlColumnType::Integer),
            "bigint" => Some(SqlColumnType::BigInt),
            "numeric" | "decimal" => {
                let p = numeric_precision
                    .and_then(|val| u8::try_from(val).ok())
                    .unwrap_or(10);
                let s = numeric_scale
                    .and_then(|val| u8::try_from(val).ok())
                    .unwrap_or(2);
                Some(SqlColumnType::Decimal(p, s))
            }
            "boolean" => Some(SqlColumnType::Boolean),
            "date" => Some(SqlColumnType::Date),
            "timestamp with time zone" | "timestamptz" => Some(SqlColumnType::Timestamptz),
            "jsonb" => Some(SqlColumnType::Jsonb),
            _ => None,
        }
    }
}

impl ColumnDefinition {
    #[cfg(test)]
    pub(super) fn new(
        name: impl Into<String>,
        data_type: SqlColumnType,
        nullable: bool,
        is_primary_key: bool,
    ) -> Self {
        Self {
            name: name.into(),
            data_type,
            nullable,
            is_primary_key,
            default_value: None,
        }
    }
}

impl IndexDefinition {
    pub(super) fn new(
        name: impl Into<String>,
        table_name: impl Into<String>,
        columns: Vec<String>,
        unique: bool,
    ) -> Self {
        Self {
            name: name.into(),
            table_name: table_name.into(),
            columns,
            unique,
        }
    }
}

impl ForeignKeyDefinition {
    pub(super) fn new(
        name: impl Into<String>,
        columns: Vec<String>,
        referenced_table: impl Into<String>,
        referenced_columns: Vec<String>,
        on_delete: ForeignKeyAction,
        on_update: ForeignKeyAction,
    ) -> Self {
        Self {
            name: name.into(),
            columns,
            referenced_table: referenced_table.into(),
            referenced_columns,
            on_delete,
            on_update,
        }
    }
}

impl TableDefinition {
    pub(super) fn new(name: impl Into<String>, kind: TableKind) -> Self {
        Self {
            name: name.into(),
            columns: IndexSet::new(),
            indexes: IndexSet::new(),
            foreign_keys: IndexSet::new(),
            kind,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn is_junction(&self) -> bool {
        self.kind == TableKind::Link
    }

    /// Returns the column names that constitute this table's primary key.
    #[must_use]
    pub(super) fn primary_key_columns(&self) -> Vec<&str> {
        self.columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.as_str())
            .collect()
    }

    /// Returns the set of referenced table names across all foreign keys on this table.
    #[must_use]
    pub(super) fn referenced_table_names(&self) -> HashSet<&str> {
        self.foreign_keys
            .iter()
            .map(|fk| fk.referenced_table.as_str())
            .collect()
    }

    #[must_use]
    pub(super) fn find_column(&self, name: &str) -> Option<&ColumnDefinition> {
        self.columns.get(name)
    }

    #[must_use]
    pub(super) fn find_index(&self, name: &str) -> Option<&IndexDefinition> {
        self.indexes.get(name)
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn find_foreign_key(&self, name: &str) -> Option<&ForeignKeyDefinition> {
        self.foreign_keys.get(name)
    }
}

impl DatabaseSchema {
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            tables: IndexSet::new(),
        }
    }

    #[must_use]
    pub(super) fn find_table(&self, name: &str) -> Option<&TableDefinition> {
        self.tables.get(name)
    }

    pub(super) fn insert_table(&mut self, table: TableDefinition) -> bool {
        self.tables.insert(table)
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl From<domain::schema::FieldType> for SqlColumnType {
    fn from(field_type: domain::schema::FieldType) -> Self {
        use domain::schema::{FieldType, IntegerSize, PrimitiveType};
        match field_type {
            FieldType::Primitive(prim) => match prim {
                PrimitiveType::Uid => Self::Varchar(Some(255)),
                PrimitiveType::Uuid => Self::Uuid,
                PrimitiveType::Text => Self::Text,
                PrimitiveType::Integer(size) => match size {
                    IntegerSize::I16 => Self::SmallInt,
                    IntegerSize::I32 => Self::Integer,
                    IntegerSize::I64 => Self::BigInt,
                },
                PrimitiveType::Decimal { precision, scale } => Self::Decimal(precision, scale),
                PrimitiveType::Date => Self::Date,
                PrimitiveType::DateTime => Self::Timestamptz,
                PrimitiveType::Boolean => Self::Boolean,
            },
            FieldType::Email => Self::Varchar(Some(320)),
            FieldType::Url => Self::Text,
            FieldType::Json | FieldType::LocalizedText => Self::Jsonb,
        }
    }
}

impl PartialEq for ColumnDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ColumnDefinition {}

impl Hash for ColumnDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for ColumnDefinition {
    fn borrow(&self) -> &str {
        &self.name
    }
}

impl PartialEq for IndexDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for IndexDefinition {}

impl Hash for IndexDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for IndexDefinition {
    fn borrow(&self) -> &str {
        &self.name
    }
}

impl PartialEq for ForeignKeyDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for ForeignKeyDefinition {}

impl Hash for ForeignKeyDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for ForeignKeyDefinition {
    fn borrow(&self) -> &str {
        &self.name
    }
}

impl PartialEq for TableDefinition {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for TableDefinition {}

impl Hash for TableDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Borrow<str> for TableDefinition {
    fn borrow(&self) -> &str {
        &self.name
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_definition_borrow_lookup() {
        let mut tables = IndexSet::new();
        let table = TableDefinition::new("articles", TableKind::Entity);
        tables.insert(table);

        assert!(tables.contains("articles"));
        assert!(!tables.contains("authors"));

        let found = tables.get("articles");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "articles");
        assert_eq!(found.unwrap().kind, TableKind::Entity);
        assert!(!found.unwrap().is_junction());
    }

    #[test]
    fn test_index_definition_borrow_lookup() {
        let mut indexes = IndexSet::new();
        let idx = IndexDefinition::new(
            "idx_articles_title",
            "articles",
            vec!["title".into()],
            false,
        );
        indexes.insert(idx);

        assert!(indexes.contains("idx_articles_title"));
        assert!(!indexes.contains("idx_other"));

        let found = indexes.get("idx_articles_title");
        assert!(found.is_some());
        assert_eq!(found.unwrap().table_name, "articles");
    }

    #[test]
    fn test_foreign_key_definition_borrow_lookup() {
        let mut fks = IndexSet::new();
        let fk = ForeignKeyDefinition::new(
            "fk_articles_author",
            vec!["author_id".into()],
            "authors",
            vec!["id".into()],
            ForeignKeyAction::Cascade,
            ForeignKeyAction::Cascade,
        );
        fks.insert(fk);

        assert!(fks.contains("fk_articles_author"));
        assert!(!fks.contains("fk_other"));

        let found = fks.get("fk_articles_author");
        assert!(found.is_some());
        assert_eq!(found.unwrap().referenced_table, "authors");
    }

    #[test]
    fn test_sql_type_formatting() {
        assert_eq!(SqlColumnType::Uuid.to_sql_string(), "UUID");
        assert_eq!(
            SqlColumnType::Varchar(Some(255)).to_sql_string(),
            "VARCHAR(255)"
        );
        assert_eq!(SqlColumnType::Varchar(None).to_sql_string(), "VARCHAR");
        assert_eq!(SqlColumnType::Text.to_sql_string(), "TEXT");
        assert_eq!(SqlColumnType::SmallInt.to_sql_string(), "SMALLINT");
        assert_eq!(SqlColumnType::Integer.to_sql_string(), "INTEGER");
        assert_eq!(SqlColumnType::BigInt.to_sql_string(), "BIGINT");
        assert_eq!(
            SqlColumnType::Decimal(10, 2).to_sql_string(),
            "NUMERIC(10, 2)"
        );
        assert_eq!(SqlColumnType::Boolean.to_sql_string(), "BOOLEAN");
        assert_eq!(SqlColumnType::Date.to_sql_string(), "DATE");
        assert_eq!(SqlColumnType::Timestamptz.to_sql_string(), "TIMESTAMPTZ");
        assert_eq!(SqlColumnType::Jsonb.to_sql_string(), "JSONB");
    }

    #[test]
    fn test_from_information_schema() {
        assert_eq!(
            SqlColumnType::from_information_schema("uuid", None, None, None),
            Some(SqlColumnType::Uuid)
        );
        assert_eq!(
            SqlColumnType::from_information_schema("character varying", Some(128), None, None),
            Some(SqlColumnType::Varchar(Some(128)))
        );
        assert_eq!(
            SqlColumnType::from_information_schema("numeric", None, Some(14), Some(4)),
            Some(SqlColumnType::Decimal(14, 4))
        );
        assert_eq!(
            SqlColumnType::from_information_schema("unknown_type", None, None, None),
            None
        );
    }

    #[test]
    fn test_from_domain_field_type() {
        use domain::schema::{FieldType, IntegerSize, PrimitiveType};

        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Uid)),
            SqlColumnType::Varchar(Some(255))
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Uuid)),
            SqlColumnType::Uuid
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Text)),
            SqlColumnType::Text
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I16))),
            SqlColumnType::SmallInt
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32))),
            SqlColumnType::Integer
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I64))),
            SqlColumnType::BigInt
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Decimal {
                precision: 10,
                scale: 2
            })),
            SqlColumnType::Decimal(10, 2)
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Date)),
            SqlColumnType::Date
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::DateTime)),
            SqlColumnType::Timestamptz
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Primitive(PrimitiveType::Boolean)),
            SqlColumnType::Boolean
        );
        assert_eq!(
            SqlColumnType::from(FieldType::Email),
            SqlColumnType::Varchar(Some(320))
        );
        assert_eq!(SqlColumnType::from(FieldType::Url), SqlColumnType::Text);
        assert_eq!(SqlColumnType::from(FieldType::Json), SqlColumnType::Jsonb);
        assert_eq!(
            SqlColumnType::from(FieldType::LocalizedText),
            SqlColumnType::Jsonb
        );
    }
}
