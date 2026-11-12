//! Reserved SQL and database keywords for identifier validation.

/// List of reserved SQL and PostgreSQL keywords that cannot be used as unquoted table or column identifiers.
const RESERVED_SQL_KEYWORDS: &[&str] = &[
    "all",
    "alter",
    "and",
    "as",
    "asc",
    "begin",
    "by",
    "case",
    "check",
    "column",
    "commit",
    "constraint",
    "create",
    "cross",
    "current_date",
    "current_time",
    "current_timestamp",
    "default",
    "delete",
    "desc",
    "distinct",
    "do",
    "drop",
    "else",
    "end",
    "except",
    "exists",
    "false",
    "fetch",
    "for",
    "foreign",
    "from",
    "full",
    "grant",
    "group",
    "having",
    "ilike",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "left",
    "like",
    "limit",
    "natural",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "references",
    "revoke",
    "right",
    "rollback",
    "select",
    "table",
    "then",
    "to",
    "true",
    "union",
    "unique",
    "update",
    "user",
    "values",
    "when",
    "where",
    "with",
];

/// Checks if an identifier is a reserved SQL/PostgreSQL keyword (case-insensitive).
#[must_use]
pub fn is_reserved_sql_keyword(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    RESERVED_SQL_KEYWORDS.contains(&lower.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_reserved_sql_keyword() {
        assert!(is_reserved_sql_keyword("select"));
        assert!(is_reserved_sql_keyword("SELECT"));
        assert!(is_reserved_sql_keyword("table"));
        assert!(!is_reserved_sql_keyword("title"));
        assert!(!is_reserved_sql_keyword("custom_field"));
    }
}
