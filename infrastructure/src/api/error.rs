//! RFC 9457 Problem Details HTTP error types and conversions.

use std::fmt;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use application::errors::{ApplicationError, InvalidParam};
use domain::errors::DomainError;

// ---------------------------------------------------------------------------
// Types First (Newspaper Style)
// ---------------------------------------------------------------------------

/// RFC 9457 compliant Problem Details representation for HTTP API error responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiProblemDetails {
    /// URI reference identifying the problem type.
    #[serde(rename = "type")]
    pub problem_type: String,
    /// Short human-readable summary of the problem type (e.g. "Conflict", "Not Found").
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// Stable, machine-readable application error code.
    pub code: String,
    /// Human-readable explanation of this specific occurrence (English fallback).
    pub detail: String,
    /// Optional URI reference identifying the specific occurrence of the problem.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Optional structured metadata for UI localization and programmatic API clients.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Optional list of field-level validation errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_params: Option<Vec<InvalidParam>>,
}

/// Unified API error type wrapping application domain errors and HTTP transport rejections.
#[derive(Debug)]
pub struct ApiError {
    /// Inner classification of the error condition.
    pub kind: ApiErrorKind,
    /// Optional URI path where the error occurred (RFC 9457 `instance`).
    pub instance: Option<String>,
}

/// Underlying kind of an [`ApiError`].
#[derive(Debug)]
pub enum ApiErrorKind {
    /// Application-level domain, authorization, validation, or conflict error.
    Application(ApplicationError),
    /// Unauthenticated request (e.g. missing, expired, or invalid token).
    Unauthenticated {
        /// Stable error code (e.g. "unauthenticated" or "invalid_token").
        code: &'static str,
        /// Human-readable explanation.
        detail: String,
    },
    /// Malformed or unprocessable request payload.
    BadRequest {
        /// Stable error code.
        code: &'static str,
        /// Human-readable explanation.
        detail: String,
    },
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl ApiError {
    /// Creates a 401 Unauthorized API error.
    #[must_use]
    pub fn unauthenticated(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::Unauthenticated {
                code,
                detail: detail.into(),
            },
            instance: None,
        }
    }

    /// Creates a 400 Bad Request API error.
    #[must_use]
    pub fn bad_request(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest {
                code,
                detail: detail.into(),
            },
            instance: None,
        }
    }

    /// Attaches an RFC 9457 request URI instance path to this error.
    #[must_use]
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// Returns the HTTP status code corresponding to this error.
    #[must_use]
    pub fn status_code(&self) -> StatusCode {
        match &self.kind {
            ApiErrorKind::Application(err) => StatusCode::from_u16(err.status_code())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            ApiErrorKind::Unauthenticated { .. } => StatusCode::UNAUTHORIZED,
            ApiErrorKind::BadRequest { .. } => StatusCode::BAD_REQUEST,
        }
    }

    /// Converts this error into an RFC 9457 [`ApiProblemDetails`] payload.
    #[must_use]
    pub fn to_problem_details(&self) -> ApiProblemDetails {
        match &self.kind {
            ApiErrorKind::Application(err) => {
                let prob = err.to_problem_details();
                ApiProblemDetails {
                    problem_type: prob.problem_type,
                    title: prob.title,
                    status: prob.status,
                    code: prob.code,
                    detail: prob.detail,
                    instance: self.instance.clone(),
                    details: prob.details,
                    invalid_params: prob.invalid_params,
                }
            }
            ApiErrorKind::Unauthenticated { code, detail } => ApiProblemDetails {
                problem_type: format!("urn:luminair:error:{code}"),
                title: "Unauthorized".to_string(),
                status: StatusCode::UNAUTHORIZED.as_u16(),
                code: (*code).to_string(),
                detail: detail.clone(),
                instance: self.instance.clone(),
                details: None,
                invalid_params: None,
            },
            ApiErrorKind::BadRequest { code, detail } => ApiProblemDetails {
                problem_type: format!("urn:luminair:error:{code}"),
                title: "Bad Request".to_string(),
                status: StatusCode::BAD_REQUEST.as_u16(),
                code: (*code).to_string(),
                detail: detail.clone(),
                instance: self.instance.clone(),
                details: None,
                invalid_params: None,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.instance {
            Some(instance) => write!(f, "{} (at {instance})", self.kind),
            None => write!(f, "{}", self.kind),
        }
    }
}

impl std::error::Error for ApiError {}

impl fmt::Display for ApiErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Application(err) => write!(f, "{err}"),
            Self::Unauthenticated { code, detail } => {
                write!(f, "unauthenticated [{code}]: {detail}")
            }
            Self::BadRequest { code, detail } => write!(f, "bad request [{code}]: {detail}"),
        }
    }
}

impl From<ApplicationError> for ApiError {
    fn from(err: ApplicationError) -> Self {
        Self {
            kind: ApiErrorKind::Application(err),
            instance: None,
        }
    }
}

impl From<DomainError> for ApiError {
    fn from(err: DomainError) -> Self {
        Self::from(ApplicationError::from(err))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let problem = self.to_problem_details();
        (
            status,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/problem+json"),
            )],
            Json(problem),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use application::errors::ConflictCode;
    use axum::body::to_bytes;
    use domain::auth::{ActorId, Permission, UserId};
    use domain::content::DocumentInstanceId;

    #[tokio::test]
    async fn test_401_unauthenticated_response() {
        let err = ApiError::unauthenticated("unauthenticated", "Bearer token missing")
            .with_instance("/api/v1/content/articles");

        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);

        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .expect("content-type header"),
            "application/problem+json"
        );

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let problem: ApiProblemDetails =
            serde_json::from_slice(&bytes).expect("deserialize problem details");

        assert_eq!(problem.status, 401);
        assert_eq!(problem.code, "unauthenticated");
        assert_eq!(problem.title, "Unauthorized");
        assert_eq!(problem.detail, "Bearer token missing");
        assert_eq!(
            problem.instance.as_deref(),
            Some("/api/v1/content/articles")
        );
        assert_eq!(problem.problem_type, "urn:luminair:error:unauthenticated");
    }

    #[tokio::test]
    async fn test_400_bad_request_response() {
        let err = ApiError::bad_request("malformed_payload", "JSON payload is corrupt");
        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);

        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let problem: ApiProblemDetails =
            serde_json::from_slice(&bytes).expect("deserialize problem details");

        assert_eq!(problem.status, 400);
        assert_eq!(problem.code, "malformed_payload");
        assert_eq!(problem.title, "Bad Request");
    }

    #[tokio::test]
    async fn test_403_unauthorized_response() {
        let user_id = UserId::try_new("user-42").expect("valid user id");
        let app_err = ApplicationError::Unauthorized {
            actor: ActorId::User(user_id),
            action: Permission::ManageSchema,
        };
        let err = ApiError::from(app_err);

        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let problem: ApiProblemDetails =
            serde_json::from_slice(&bytes).expect("deserialize problem details");

        assert_eq!(problem.status, 403);
        assert_eq!(problem.code, "unauthorized");
        assert_eq!(problem.title, "Forbidden");
        assert!(problem.details.is_some());
    }

    #[tokio::test]
    async fn test_404_not_found_response() {
        let app_err = ApplicationError::NotFound {
            entity: "Article",
            id: "01946890-7812-789a-bcde-000000000001".to_string(),
        };
        let err = ApiError::from(app_err);

        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let problem: ApiProblemDetails =
            serde_json::from_slice(&bytes).expect("deserialize problem details");

        assert_eq!(problem.status, 404);
        assert_eq!(problem.code, "resource_not_found");
        assert_eq!(problem.title, "Not Found");
    }

    #[tokio::test]
    async fn test_409_conflict_response() {
        let app_err = ApplicationError::conflict(
            ConflictCode::VersionMismatch,
            "version mismatch: expected 1, found 2",
        );
        let err = ApiError::from(app_err);

        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let problem: ApiProblemDetails =
            serde_json::from_slice(&bytes).expect("deserialize problem details");

        assert_eq!(problem.status, 409);
        assert_eq!(problem.code, "version_mismatch");
        assert_eq!(problem.title, "Conflict");
    }

    #[tokio::test]
    async fn test_422_validation_with_invalid_params() {
        let app_err = ApplicationError::Validation(vec![
            InvalidParam::new("title", "title is required", "required_field_missing"),
            InvalidParam::new("locale", "locale 'xx' is unsupported", "unknown_locale"),
        ]);
        let err = ApiError::from(app_err);

        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let problem: ApiProblemDetails =
            serde_json::from_slice(&bytes).expect("deserialize problem details");

        assert_eq!(problem.status, 422);
        assert_eq!(problem.code, "validation_failed");
        assert_eq!(problem.title, "Unprocessable Entity");

        let params = problem.invalid_params.expect("invalid_params present");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "title");
        assert_eq!(params[0].code, "required_field_missing");
        assert_eq!(params[1].name, "locale");
        assert_eq!(params[1].code, "unknown_locale");
    }

    #[tokio::test]
    async fn test_500_internal_error_response() {
        let app_err = ApplicationError::Internal("database connection timeout".into());
        let err = ApiError::from(app_err);

        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("read body");
        let problem: ApiProblemDetails =
            serde_json::from_slice(&bytes).expect("deserialize problem details");

        assert_eq!(problem.status, 500);
        assert_eq!(problem.code, "internal_error");
        assert_eq!(problem.title, "Internal Server Error");
    }

    #[test]
    fn test_from_domain_error() {
        let doc_id = DocumentInstanceId::new(uuid::Uuid::now_v7());
        let domain_err = DomainError::DocumentInstanceNotFound(doc_id);
        let api_err = ApiError::from(domain_err);

        assert_eq!(api_err.status_code(), StatusCode::NOT_FOUND);
        let problem = api_err.to_problem_details();
        assert_eq!(problem.code, "document_instance_not_found");
        assert_eq!(problem.status, 404);
    }
}
