//! Axum HTTP middleware for tracing request IDs and injecting RFC 9457 problem instances.

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants First (Newspaper Style)
// ---------------------------------------------------------------------------

const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");
const PROBLEM_CONTENT_TYPE_PREFIX: &str = "application/problem+json";

// ---------------------------------------------------------------------------
// Public Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Middleware that extracts or generates a unique `x-request-id` and propagates it into response headers.
pub async fn propagate_request_id(req: Request, next: Next) -> Response {
    let request_id = extract_or_generate_request_id(req.headers().get(&X_REQUEST_ID));
    let mut res = next.run(req).await;
    if let Ok(header_val) = HeaderValue::try_from(&request_id) {
        res.headers_mut().insert(X_REQUEST_ID, header_val);
    }
    res
}

/// Middleware that injects the request URI path as the RFC 9457 `instance` field if missing.
pub async fn inject_problem_instance(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let res = next.run(req).await;
    maybe_inject_instance(res, &path).await
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn extract_or_generate_request_id(header_val: Option<&HeaderValue>) -> String {
    header_val
        .and_then(|val| val.to_str().ok())
        .filter(|s| !s.trim().is_empty())
        .map_or_else(|| Uuid::now_v7().to_string(), ToString::to_string)
}

async fn maybe_inject_instance(res: Response, path: &str) -> Response {
    let is_problem = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|val| val.to_str().ok())
        .is_some_and(|ct| ct.starts_with(PROBLEM_CONTENT_TYPE_PREFIX));

    if !is_problem {
        return res;
    }

    inject_instance_into_body(res, path).await
}

async fn inject_instance_into_body(res: Response, path: &str) -> Response {
    let (mut parts, body) = res.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, 1024 * 1024).await else {
        return Response::from_parts(parts, axum::body::Body::empty());
    };

    let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Response::from_parts(parts, axum::body::Body::from(bytes));
    };

    if let Some(obj) = json.as_object_mut()
        && (!obj.contains_key("instance")
            || obj.get("instance").is_some_and(serde_json::Value::is_null))
    {
        obj.insert(
            "instance".to_string(),
            serde_json::Value::String(path.to_string()),
        );
        if let Ok(updated_bytes) = serde_json::to_vec(&json) {
            parts.headers.remove(header::CONTENT_LENGTH);
            return Response::from_parts(parts, axum::body::Body::from(updated_bytes));
        }
    }

    Response::from_parts(parts, axum::body::Body::from(bytes))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::StatusCode;
    use axum::middleware::from_fn;
    use axum::routing::get;
    use axum_test::TestServer;

    #[tokio::test]
    async fn test_propagate_request_id_generates_new_uuid() {
        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(from_fn(propagate_request_id));

        let server = TestServer::new(app);
        let res = server.get("/test").await;

        let req_id = res
            .headers()
            .get(&X_REQUEST_ID)
            .expect("x-request-id present")
            .to_str()
            .expect("valid str");
        assert!(Uuid::parse_str(req_id).is_ok());
    }

    #[tokio::test]
    async fn test_propagate_request_id_preserves_existing() {
        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(from_fn(propagate_request_id));

        let server = TestServer::new(app);
        let res = server
            .get("/test")
            .add_header(&X_REQUEST_ID, "custom-trace-id-999")
            .await;

        let req_id = res
            .headers()
            .get(&X_REQUEST_ID)
            .expect("x-request-id present")
            .to_str()
            .expect("valid str");
        assert_eq!(req_id, "custom-trace-id-999");
    }

    #[tokio::test]
    async fn test_inject_problem_instance_adds_missing_instance() {
        let app = Router::new()
            .route(
                "/api/v1/content/articles",
                get(|| async {
                    (
                        StatusCode::NOT_FOUND,
                        [(
                            header::CONTENT_TYPE,
                            "application/problem+json; charset=utf-8",
                        )],
                        serde_json::json!({
                            "type": "urn:luminair:error:resource_not_found",
                            "title": "Not Found",
                            "status": 404,
                            "code": "resource_not_found",
                            "detail": "Resource not found"
                        })
                        .to_string(),
                    )
                }),
            )
            .layer(from_fn(inject_problem_instance));

        let server = TestServer::new(app);
        let res = server.get("/api/v1/content/articles").await;

        let json = res.json::<serde_json::Value>();
        assert_eq!(json["instance"], "/api/v1/content/articles");
    }

    #[tokio::test]
    async fn test_inject_problem_instance_preserves_existing_instance() {
        let app = Router::new()
            .route(
                "/api/v1/content/articles",
                get(|| async {
                    (
                        StatusCode::NOT_FOUND,
                        [(header::CONTENT_TYPE, "application/problem+json")],
                        serde_json::json!({
                            "type": "urn:luminair:error:resource_not_found",
                            "title": "Not Found",
                            "status": 404,
                            "code": "resource_not_found",
                            "detail": "Resource not found",
                            "instance": "/original/instance/path"
                        })
                        .to_string(),
                    )
                }),
            )
            .layer(from_fn(inject_problem_instance));

        let server = TestServer::new(app);
        let res = server.get("/api/v1/content/articles").await;

        let json = res.json::<serde_json::Value>();
        assert_eq!(json["instance"], "/original/instance/path");
    }

    #[tokio::test]
    async fn test_inject_problem_instance_ignores_non_problem_responses() {
        let app = Router::new()
            .route(
                "/api/v1/system/config",
                get(|| async {
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "application/json")],
                        serde_json::json!({ "data": "ok" }).to_string(),
                    )
                }),
            )
            .layer(from_fn(inject_problem_instance));

        let server = TestServer::new(app);
        let res = server.get("/api/v1/system/config").await;

        let json = res.json::<serde_json::Value>();
        assert!(json.get("instance").is_none());
        assert_eq!(json["data"], "ok");
    }
}
