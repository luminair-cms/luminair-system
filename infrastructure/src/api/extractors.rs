//! Query and path parameter extractors and parsers for HTTP REST endpoints.

use serde::Deserialize;
use uuid::Uuid;

use application::errors::{ApplicationError, InvalidParam};
use domain::auth::AccessRequestId;
use domain::content::header::{DocumentStatus, VersionView};
use domain::content::instance::DocumentInstanceId;
use domain::content::ports::Pagination;
use domain::errors::DomainError;
use domain::schema::{AttributeId, DocumentType, DocumentTypeId};
use domain::system::SystemContext;

use crate::api::error::ApiError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_PAGE: u32 = 1;
const DEFAULT_PAGE_SIZE: u32 = 20;
const MIN_PAGE_SIZE: u32 = 1;
const MAX_PAGE_SIZE: u32 = 100;

// ---------------------------------------------------------------------------
// Query Structs
// ---------------------------------------------------------------------------

/// Standard pagination query parameters (`?page=1&pageSize=20`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// Query parameters for listing document instances (`GET /api/v1/content/:type_id`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindDocumentsQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub view: Option<String>,
    pub populate: Option<String>,
}

/// Query parameters for fetching a single document instance (`GET /api/v1/content/:type_id/:id`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindByIdQuery {
    pub view: Option<String>,
    pub populate: Option<String>,
}

/// Query parameters for admin studio header views (`GET /api/v1/content/:type_id/admin/headers`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminHeadersQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub status: Option<DocumentStatus>,
}

/// Query parameters for listing user profiles in the directory (`GET /api/v1/auth/users`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListUsersQuery {
    pub q: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl PaginationQuery {
    /// Extracts bounded domain [`Pagination`] from the query.
    #[must_use]
    pub fn pagination(&self) -> Pagination {
        compute_bounded_pagination(self.page, self.page_size)
    }

    /// Converts optional page and page_size parameters into bounded domain [`Pagination`].
    #[must_use]
    pub fn into_pagination(self) -> Pagination {
        self.pagination()
    }
}

impl ListUsersQuery {
    /// Extracts bounded domain [`Pagination`] from the query.
    #[must_use]
    pub fn pagination(&self) -> Pagination {
        compute_bounded_pagination(self.page, self.page_size)
    }
}

impl FindDocumentsQuery {
    /// Extracts bounded domain [`Pagination`] from the query.
    #[must_use]
    pub fn pagination(&self) -> Pagination {
        compute_bounded_pagination(self.page, self.page_size)
    }

    /// Parses the requested document lifecycle view, falling back to `default_view`.
    ///
    /// # Errors
    /// Returns [`ApiError`] validation failure if the view name is unrecognized.
    pub fn parse_view(&self, default_view: VersionView) -> Result<VersionView, ApiError> {
        parse_view_param(self.view.as_deref(), default_view)
    }

    /// Parses the requested relational population attribute list for `doc_type`.
    ///
    /// # Errors
    /// Returns [`ApiError`] validation failure if any requested attribute is unknown.
    pub fn parse_populate(&self, doc_type: &DocumentType) -> Result<Vec<AttributeId>, ApiError> {
        parse_populate_param(self.populate.as_deref(), doc_type)
    }
}

impl FindByIdQuery {
    /// Parses the requested document lifecycle view, falling back to `default_view`.
    ///
    /// # Errors
    /// Returns [`ApiError`] validation failure if the view name is unrecognized.
    pub fn parse_view(&self, default_view: VersionView) -> Result<VersionView, ApiError> {
        parse_view_param(self.view.as_deref(), default_view)
    }

    /// Parses the requested relational population attribute list for `doc_type`.
    ///
    /// # Errors
    /// Returns [`ApiError`] validation failure if any requested attribute is unknown.
    pub fn parse_populate(&self, doc_type: &DocumentType) -> Result<Vec<AttributeId>, ApiError> {
        parse_populate_param(self.populate.as_deref(), doc_type)
    }
}

impl AdminHeadersQuery {
    /// Extracts bounded domain [`Pagination`] from the query.
    #[must_use]
    pub fn pagination(&self) -> Pagination {
        compute_bounded_pagination(self.page, self.page_size)
    }
}

// ---------------------------------------------------------------------------
// Public Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Resolves a static document type definition from the system context by identifier.
///
/// # Errors
/// Returns [`ApiError`] 404 Not Found if the document type is unknown or invalid.
pub fn resolve_document_type(
    context: &'static SystemContext,
    raw_type_id: &str,
) -> Result<&'static DocumentType, ApiError> {
    let type_id = DocumentTypeId::try_new(raw_type_id).map_err(|_| {
        ApiError::from(ApplicationError::NotFound {
            entity: "DocumentType",
            id: raw_type_id.to_string(),
        })
    })?;

    context
        .find_type(&type_id)
        .ok_or_else(|| ApiError::from(ApplicationError::Domain(DomainError::DocumentTypeNotFound(type_id))))
}

/// Parses and validates a raw document UUID string.
///
/// # Errors
/// Returns [`ApiError`] 400 Bad Request if the string is not a valid UUID.
pub fn parse_instance_id(raw_id: &str) -> Result<DocumentInstanceId, ApiError> {
    Uuid::parse_str(raw_id)
        .map(DocumentInstanceId::new)
        .map_err(|_| ApiError::bad_request("invalid_uuid", format!("invalid document UUID '{raw_id}'")))
}

/// Parses and validates a raw access request UUID string.
///
/// # Errors
/// Returns [`ApiError`] 400 Bad Request if the string is not a valid UUID.
pub fn parse_access_request_id(raw_id: &str) -> Result<AccessRequestId, ApiError> {
    Uuid::parse_str(raw_id)
        .map(AccessRequestId::new)
        .map_err(|_| {
            ApiError::bad_request(
                "invalid_uuid",
                format!("invalid access request UUID '{raw_id}'"),
            )
        })
}

/// Parses a `view` query parameter string into a domain [`VersionView`].
///
/// Supported values:
/// - `None` or empty: returns `default_view`
/// - `"published"`: [`VersionView::Published`]
/// - `"working"` | `"draft"`: [`VersionView::Working`]
///
/// # Errors
/// Returns [`ApiError`] 422 Validation error if an unrecognized view name is passed.
pub fn parse_view_param(
    raw: Option<&str>,
    default_view: VersionView,
) -> Result<VersionView, ApiError> {
    let Some(raw) = raw else {
        return Ok(default_view);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(default_view);
    }

    match trimmed {
        "published" => Ok(VersionView::Published),
        "working" | "draft" => Ok(VersionView::Working),
        other => {
            let detail = format!("invalid view '{other}': expected 'published' or 'working'");
            let param = InvalidParam::new("view", detail, "invalid_query_param");
            Err(ApiError::from(ApplicationError::Validation(vec![param])))
        }
    }
}

/// Parses a `populate` query parameter string into validated relational [`AttributeId`]s.
///
/// Supports:
/// - `None` or empty: empty list
/// - `"*"`: all relations declared on `doc_type`
/// - Comma-separated list: e.g. `"author,tags"`
///
/// # Errors
/// Returns [`ApiError`] 422 Validation error if any attribute does not exist on `doc_type`.
pub fn parse_populate_param(
    raw: Option<&str>,
    doc_type: &DocumentType,
) -> Result<Vec<AttributeId>, ApiError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed == "*" {
        return Ok(doc_type.relations.iter().map(|r| r.id.clone()).collect());
    }

    parse_populate_attribute_list(trimmed, doc_type)
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn compute_bounded_pagination(page: Option<u32>, page_size: Option<u32>) -> Pagination {
    let resolved_page = page.unwrap_or(DEFAULT_PAGE).max(1);
    let resolved_size = page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(MIN_PAGE_SIZE, MAX_PAGE_SIZE);
    Pagination {
        page: resolved_page,
        page_size: resolved_size,
    }
}

fn parse_populate_attribute_list(
    raw: &str,
    doc_type: &DocumentType,
) -> Result<Vec<AttributeId>, ApiError> {
    let mut resolved = Vec::new();
    let mut errors = Vec::new();

    for item in raw.split(',') {
        let trimmed_item = item.trim();
        if trimmed_item.is_empty() {
            continue;
        }
        validate_single_populate_item(trimmed_item, doc_type, &mut resolved, &mut errors);
    }

    if errors.is_empty() {
        Ok(resolved)
    } else {
        Err(ApiError::from(ApplicationError::Validation(errors)))
    }
}

fn validate_single_populate_item(
    item: &str,
    doc_type: &DocumentType,
    resolved: &mut Vec<AttributeId>,
    errors: &mut Vec<InvalidParam>,
) {
    let attr_id = match AttributeId::try_new(item) {
        Ok(id) => id,
        Err(_) => {
            errors.push(InvalidParam::new(
                "populate",
                format!("invalid attribute identifier '{item}'"),
                "unknown_relation",
            ));
            return;
        }
    };

    if doc_type.relations.contains(&attr_id) {
        if !resolved.contains(&attr_id) {
            resolved.push(attr_id);
        }
    } else {
        errors.push(InvalidParam::new(
            "populate",
            format!(
                "relation '{item}' does not exist on document type '{}'",
                doc_type.id
            ),
            "unknown_relation",
        ));
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::common::DisplayName;
    use domain::schema::{
        DocumentKind, DocumentTypeInfo, DocumentTypeOptions, RelationDefinition, RelationType,
        SchemaRegistry,
    };
    use domain::system::{LocaleId, SystemConfig, SystemConfigId};
    use indexmap::IndexSet;

    fn test_doc_type() -> DocumentType {
        let type_id = DocumentTypeId::try_new("articles").expect("valid type id");
        let author_rel_id = AttributeId::try_new("author").expect("valid attr");
        let tags_rel_id = AttributeId::try_new("tags").expect("valid attr");
        let users_type_id = DocumentTypeId::try_new("users").expect("valid target");

        let mut relations = IndexSet::new();
        relations.insert(RelationDefinition {
            id: author_rel_id,
            relation_type: RelationType::HasOne,
            target: users_type_id.clone(),
        });
        relations.insert(RelationDefinition {
            id: tags_rel_id,
            relation_type: RelationType::HasMany,
            target: users_type_id,
        });

        DocumentType {
            id: type_id.clone(),
            kind: DocumentKind::Collection,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Articles").expect("valid name"),
                singular_name: type_id.clone(),
                plural_name: type_id,
                description: None,
                display_fields: Vec::new(),
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields: IndexSet::new(),
            relations,
        }
    }

    fn fixture_context() -> &'static SystemContext {
        let registry = SchemaRegistry::new(vec![test_doc_type()]);
        let locale = LocaleId::try_new("en").expect("valid locale");
        let config = SystemConfig::new(
            SystemConfigId::new(uuid::Uuid::now_v7()),
            vec![locale.clone()],
            locale,
        )
        .expect("valid config");
        Box::leak(Box::new(SystemContext::new(registry, config)))
    }

    #[test]
    fn test_pagination_query_defaults_and_clamping() {
        let q = PaginationQuery {
            page: None,
            page_size: None,
        };
        assert_eq!(q.into_pagination(), Pagination { page: 1, page_size: 20 });

        let q2 = PaginationQuery {
            page: Some(0),
            page_size: Some(150),
        };
        assert_eq!(q2.into_pagination(), Pagination { page: 1, page_size: 100 });

        let q3 = PaginationQuery {
            page: Some(5),
            page_size: Some(0),
        };
        assert_eq!(q3.into_pagination(), Pagination { page: 5, page_size: 1 });
    }

    #[test]
    fn test_parse_view_param() {
        assert_eq!(
            parse_view_param(None, VersionView::Published).unwrap(),
            VersionView::Published
        );
        assert_eq!(
            parse_view_param(Some("published"), VersionView::Working).unwrap(),
            VersionView::Published
        );
        assert_eq!(
            parse_view_param(Some("working"), VersionView::Published).unwrap(),
            VersionView::Working
        );
        assert_eq!(
            parse_view_param(Some("draft"), VersionView::Published).unwrap(),
            VersionView::Working
        );

        let err = parse_view_param(Some("archived"), VersionView::Published).unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_parse_populate_param() {
        let dt = test_doc_type();

        assert!(parse_populate_param(None, &dt).unwrap().is_empty());
        assert!(parse_populate_param(Some(""), &dt).unwrap().is_empty());

        let wildcard = parse_populate_param(Some("*"), &dt).unwrap();
        assert_eq!(wildcard.len(), 2);

        let specific = parse_populate_param(Some("author,tags"), &dt).unwrap();
        assert_eq!(specific.len(), 2);
        assert_eq!(specific[0].as_ref(), "author");
        assert_eq!(specific[1].as_ref(), "tags");

        let err = parse_populate_param(Some("author,nonexistent"), &dt).unwrap_err();
        assert_eq!(err.status_code(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_resolve_document_type() {
        let context = fixture_context();
        let doc_type = resolve_document_type(context, "articles").expect("found");
        assert_eq!(doc_type.id.as_ref(), "articles");

        let err = resolve_document_type(context, "unknown_type").expect_err("not found");
        assert_eq!(err.status_code(), axum::http::StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_parse_instance_id() {
        let valid = "01946890-7812-789a-bcde-000000000001";
        let parsed = parse_instance_id(valid).expect("valid uuid");
        assert_eq!(parsed.to_string(), valid);

        let err = parse_instance_id("not-a-uuid").expect_err("invalid uuid");
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_parse_access_request_id() {
        let valid = "01946890-7812-789a-bcde-000000000001";
        let parsed = parse_access_request_id(valid).expect("valid uuid");
        assert_eq!(parsed.to_string(), valid);

        let err = parse_access_request_id("not-a-uuid").expect_err("invalid uuid");
        assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_admin_headers_query_pagination() {
        let q = AdminHeadersQuery {
            page: Some(3),
            page_size: Some(15),
            status: Some(DocumentStatus::Published),
        };
        assert_eq!(q.pagination(), Pagination { page: 3, page_size: 15 });
    }
}
