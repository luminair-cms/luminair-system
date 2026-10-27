//! Sanitization and validation utilities for database identifiers.

/// List of reserved SQL and PostgreSQL keywords that cannot be used as unquoted table or column identifiers.
pub const RESERVED_SQL_KEYWORDS: &[&str] = &[
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

/// Joins multiple string segments into a single `snake_case` identifier in a single allocation.
///
/// Any hyphens (`'-'`) within segments are replaced with underscores (`'_'`), and segments
/// are separated by underscores.
#[must_use]
pub fn join_snake_segments(segments: &[&str]) -> String {
    if segments.is_empty() {
        return String::new();
    }
    let total_len = segments.iter().map(|s| s.len()).sum::<usize>()
        + segments.len().saturating_sub(1);
    let mut result = String::with_capacity(total_len);
    for (i, segment) in segments.iter().enumerate() {
        if i > 0 {
            result.push('_');
        }
        append_sanitized_segment(&mut result, segment);
    }
    result
}

/// Converts a kebab-case identifier to `snake_case`.
/// E.g. `blog-post` -> `blog_post`, `hero-image-url` -> `hero_image_url`.
#[must_use]
pub fn kebab_to_snake(s: &str) -> String {
    join_snake_segments(&[s])
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn append_sanitized_segment(buffer: &mut String, segment: &str) {
    for c in segment.chars() {
        if c == '-' {
            buffer.push('_');
        } else {
            buffer.push(c);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_snake_segments_empty() {
        assert_eq!(join_snake_segments(&[]), "");
    }

    #[test]
    fn test_join_snake_segments_single() {
        assert_eq!(join_snake_segments(&["article"]), "article");
        assert_eq!(join_snake_segments(&["blog-post"]), "blog_post");
    }

    #[test]
    fn test_join_snake_segments_multiple() {
        assert_eq!(
            join_snake_segments(&["articles", "author-tags", "links"]),
            "articles_author_tags_links"
        );
        assert_eq!(
            join_snake_segments(&["idx", "blog-posts", "created-at"]),
            "idx_blog_posts_created_at"
        );
    }

    #[test]
    fn test_kebab_to_snake() {
        assert_eq!(kebab_to_snake("blog-post"), "blog_post");
        assert_eq!(kebab_to_snake("single"), "single");
        assert_eq!(kebab_to_snake("user-profile-avatar"), "user_profile_avatar");
    }

    #[test]
    fn test_reserved_keywords() {
        assert!(is_reserved_sql_keyword("user"));
        assert!(is_reserved_sql_keyword("USER"));
        assert!(is_reserved_sql_keyword("select"));
        assert!(!is_reserved_sql_keyword("article"));
        assert!(!is_reserved_sql_keyword("title"));
    }
}
