//! Generic HTTP response envelopes and pagination metadata wrappers.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use domain::content::Pagination;

// ---------------------------------------------------------------------------
// Types First (Newspaper Style)
// ---------------------------------------------------------------------------

/// Standard success envelope wrapping payload data: `{"data": T}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonData<T> {
    /// Inner payload data.
    pub data: T,
}

/// Resource creation envelope returning HTTP `201 Created` with payload: `{"data": T}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedJson<T> {
    /// Created resource data.
    pub data: T,
}

/// Pagination metadata accompanying paged collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationMeta {
    /// Current 1-based page number.
    pub page: u32,
    /// Number of items requested per page.
    pub page_size: u32,
    /// Total count of items matching the query.
    pub total: u64,
    /// Total number of pages available.
    pub total_pages: u32,
}

/// Paginated collection envelope returning `{"data": [...], "meta": {...}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PagedJson<T> {
    /// Current page items.
    pub data: Vec<T>,
    /// Pagination metadata.
    pub meta: PaginationMeta,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl<T> JsonData<T> {
    /// Wraps payload data in a standard success envelope.
    #[must_use]
    pub const fn new(data: T) -> Self {
        Self { data }
    }
}

impl<T> CreatedJson<T> {
    /// Wraps payload data in a 201 Created envelope.
    #[must_use]
    pub const fn new(data: T) -> Self {
        Self { data }
    }
}

impl PaginationMeta {
    /// Computes pagination metadata from requested pagination parameters and total item count.
    #[must_use]
    pub fn new(pagination: Pagination, total: u64) -> Self {
        let total_pages = calculate_total_pages(pagination.page_size, total);
        Self {
            page: pagination.page,
            page_size: pagination.page_size,
            total,
            total_pages,
        }
    }
}

impl<T> PagedJson<T> {
    /// Constructs a paginated response envelope from items, requested pagination, and total count.
    #[must_use]
    pub fn new(data: Vec<T>, pagination: Pagination, total: u64) -> Self {
        Self {
            data,
            meta: PaginationMeta::new(pagination, total),
        }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl<T> From<T> for JsonData<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

impl<T> From<T> for CreatedJson<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

impl<T: Serialize> IntoResponse for JsonData<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

impl<T: Serialize> IntoResponse for CreatedJson<T> {
    fn into_response(self) -> Response {
        (StatusCode::CREATED, Json(self)).into_response()
    }
}

impl<T: Serialize> IntoResponse for PagedJson<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn calculate_total_pages(page_size: u32, total: u64) -> u32 {
    if page_size == 0 || total == 0 {
        0
    } else {
        let page_size_u64 = u64::from(page_size);
        let pages = total.div_ceil(page_size_u64);
        u32::try_from(pages).unwrap_or(u32::MAX)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::header;

    #[tokio::test]
    async fn test_json_data_into_response() {
        let envelope = JsonData::new("hello".to_string());
        let response = envelope.into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .expect("content-type"),
            "application/json"
        );

        let bytes = to_bytes(response.into_body(), 1024).await.expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse json");
        assert_eq!(json["data"], "hello");
    }

    #[tokio::test]
    async fn test_created_json_into_response() {
        let envelope = CreatedJson::new(serde_json::json!({ "id": 123 }));
        let response = envelope.into_response();

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .expect("content-type"),
            "application/json"
        );

        let bytes = to_bytes(response.into_body(), 1024).await.expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse json");
        assert_eq!(json["data"]["id"], 123);
    }

    #[tokio::test]
    async fn test_paged_json_into_response() {
        let pagination = Pagination {
            page: 2,
            page_size: 10,
        };
        let items = vec!["item1".to_string(), "item2".to_string()];
        let envelope = PagedJson::new(items, pagination, 25);
        let response = envelope.into_response();

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), 1024).await.expect("body");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse json");

        assert_eq!(json["data"].as_array().expect("array").len(), 2);
        assert_eq!(json["meta"]["page"], 2);
        assert_eq!(json["meta"]["pageSize"], 10);
        assert_eq!(json["meta"]["total"], 25);
        assert_eq!(json["meta"]["totalPages"], 3);
    }

    #[test]
    fn test_pagination_meta_calculations() {
        // Zero items
        let p0 = PaginationMeta::new(
            Pagination {
                page: 1,
                page_size: 25,
            },
            0,
        );
        assert_eq!(p0.total_pages, 0);

        // Exact multiple
        let p1 = PaginationMeta::new(
            Pagination {
                page: 1,
                page_size: 20,
            },
            40,
        );
        assert_eq!(p1.total_pages, 2);

        // Multiple with remainder
        let p2 = PaginationMeta::new(
            Pagination {
                page: 1,
                page_size: 20,
            },
            41,
        );
        assert_eq!(p2.total_pages, 3);

        // Zero page_size guard
        let p3 = PaginationMeta::new(
            Pagination {
                page: 1,
                page_size: 0,
            },
            100,
        );
        assert_eq!(p3.total_pages, 0);
    }
}
