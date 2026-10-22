//! Application-level error types and RFC 9457 Problem Details mapping.

use domain::auth::{ActorId, Permission};
use domain::content::DocumentInstanceId;
use domain::errors::DomainError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Machine-readable conflict error code for programmatic handling and UI localization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictCode {
    /// The document has pending working draft edits that would be overwritten.
    DraftHasPendingEdits,
    /// The document type does not support the draft-and-publish workflow.
    DraftAndPublishDisabled,
    /// The document is already published with no pending draft changes to publish.
    AlreadyPublished,
    /// Optimistic concurrency control failure: expected version did not match current version.
    VersionMismatch,
    /// A single-type instance already exists for this document type.
    SingleTypeAlreadyExists,
    /// A generic conflict condition occurred.
    GenericConflict,
}

impl ConflictCode {
    /// Returns the static snake_case string representation of the conflict code.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::DraftHasPendingEdits => "draft_has_pending_edits",
            Self::DraftAndPublishDisabled => "draft_and_publish_disabled",
            Self::AlreadyPublished => "already_published",
            Self::VersionMismatch => "version_mismatch",
            Self::SingleTypeAlreadyExists => "single_type_already_exists",
            Self::GenericConflict => "generic_conflict",
        }
    }
}

impl std::fmt::Display for ConflictCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A specific invalid field or parameter description within a validation error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidParam {
    /// Parameter or attribute name.
    pub name: String,
    /// Reason explaining why the parameter was rejected.
    pub reason: String,
    /// Machine-readable code for the specific parameter violation.
    pub code: String,
}

/// RFC 9457 compliant Problem Details representation for HTTP API error responses.
///
/// NOTE: In accordance with Hexagonal Architecture (Ports & Adapters), this HTTP transport
/// representation is temporarily located in the application crate and will be moved to the
/// `infrastructure` crate (HTTP driving adapter) during the HTTP API implementation milestone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemDetails {
    /// URI reference identifying the problem type.
    #[serde(rename = "type")]
    pub problem_type: String,
    /// Short human-readable summary of the problem type (e.g. "Conflict", "Not Found").
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// Stable, machine-readable application error code (e.g. "draft_has_pending_edits").
    pub code: String,
    /// Human-readable explanation of this specific occurrence (English fallback).
    pub detail: String,
    /// Optional structured metadata for UI localization and programmatic API clients.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Optional list of field-level validation errors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_params: Option<Vec<InvalidParam>>,
}

/// Errors that can occur within the application layer use cases.
#[derive(Debug, Error)]
pub enum ApplicationError {
    /// An underlying domain rule was violated.
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),

    /// The caller lacks the required permission to perform the action.
    #[error("unauthorized: actor '{actor}' lacks permission for action '{action:?}'")]
    Unauthorized {
        /// Identifier of the actor attempting the action.
        actor: ActorId,
        /// The specific permission that was required.
        action: Permission,
    },

    /// A requested resource was not found.
    #[error("resource not found: {entity} with id '{id}'")]
    NotFound {
        /// Name of the entity type that could not be found.
        entity: &'static str,
        /// The identifier that was looked up.
        id: String,
    },

    /// An input command or query failed application-level validation.
    /// Contains all validation messages — never truncated to the first error.
    #[error("validation error: {}", .0.join("; "))]
    Validation(Vec<String>),

    /// The requested operation conflicts with existing system state.
    #[error("conflict [{code}]: {message}")]
    Conflict {
        /// Standardized machine-readable conflict code.
        code: ConflictCode,
        /// Human-readable explanation (for developer debugging/logs).
        message: String,
        /// Optional structured metadata for UI localization and programmatic API clients.
        details: Option<serde_json::Value>,
    },

    /// An internal system failure or unexpected error occurred.
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApplicationError {
    /// Creates a generic `Conflict` error with code and message.
    pub fn conflict(code: ConflictCode, message: impl Into<String>) -> Self {
        Self::Conflict {
            code,
            message: message.into(),
            details: None,
        }
    }

    /// Creates a `Conflict` error with code, message, and structured metadata.
    pub fn conflict_with_details(
        code: ConflictCode,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self::Conflict {
            code,
            message: message.into(),
            details: Some(details),
        }
    }

    /// Convenience constructor for restoring a snapshot over pending working draft edits.
    pub fn draft_overwrite_conflict(
        document_id: DocumentInstanceId,
        target_revision: u32,
        draft_version: u32,
    ) -> Self {
        Self::Conflict {
            code: ConflictCode::DraftHasPendingEdits,
            message: "restoring snapshot would overwrite pending working draft edits; confirm with discard_pending_edits: true".to_string(),
            details: Some(serde_json::json!({
                "document_id": document_id.to_string(),
                "target_revision": target_revision,
                "draft_version": draft_version,
            })),
        }
    }

    /// Convenience constructor for publishing a clean published document with no changes.
    pub fn already_published_conflict(document_id: DocumentInstanceId) -> Self {
        Self::Conflict {
            code: ConflictCode::AlreadyPublished,
            message: format!("document '{document_id}' is already published with no pending changes"),
            details: Some(serde_json::json!({
                "document_id": document_id.to_string(),
            })),
        }
    }

    /// Convenience constructor for publish/unpublish when draft_and_publish is disabled.
    pub fn draft_publish_disabled_conflict(type_title: &str) -> Self {
        Self::Conflict {
            code: ConflictCode::DraftAndPublishDisabled,
            message: format!("document type '{type_title}' does not support draft-and-publish workflow"),
            details: Some(serde_json::json!({
                "document_type": type_title,
            })),
        }
    }

    /// Convenience constructor for optimistic concurrency control version mismatch.
    pub fn version_mismatch_conflict(expected: u32, found: u32) -> Self {
        Self::Conflict {
            code: ConflictCode::VersionMismatch,
            message: format!("version mismatch: expected {expected}, found {found}"),
            details: Some(serde_json::json!({
                "expected_version": expected,
                "found_version": found,
            })),
        }
    }

    /// Returns the canonical machine-readable snake_case error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Domain(d) => match d {
                DomainError::DocumentTypeNotFound(_) => "document_type_not_found",
                DomainError::DocumentInstanceNotFound(_) => "document_instance_not_found",
                DomainError::SingleTypeAlreadyExists(_) => "single_type_already_exists",
                DomainError::InvalidFieldValue { .. } => "invalid_field_value",
                DomainError::UnknownLocale(_) => "unknown_locale",
                DomainError::UnknownAttribute(_) => "unknown_attribute",
                DomainError::AccessRequestNotFound(_) => "access_request_not_found",
                DomainError::AccessRequestAlreadyActive(_) => "access_request_already_active",
                DomainError::UserAlreadyEnrolled(_) => "user_already_enrolled",
                DomainError::Unauthorized(_) => "unauthorized",
                DomainError::InvalidStateTransition { .. } => "invalid_state_transition",
                DomainError::InvalidSchemaDisplayField { .. } => "invalid_schema_display_field",
                DomainError::Validation(_) => "validation_failed",
                DomainError::Storage(_) => "storage_error",
            },
            Self::Unauthorized { .. } => "unauthorized",
            Self::NotFound { .. } => "resource_not_found",
            Self::Validation(_) => "validation_failed",
            Self::Conflict { code, .. } => code.as_str(),
            Self::Internal(_) => "internal_error",
        }
    }

    /// Returns the HTTP status code recommended for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::Domain(d) => match d {
                DomainError::DocumentTypeNotFound(_)
                | DomainError::DocumentInstanceNotFound(_)
                | DomainError::AccessRequestNotFound(_) => 404,
                DomainError::SingleTypeAlreadyExists(_)
                | DomainError::AccessRequestAlreadyActive(_)
                | DomainError::UserAlreadyEnrolled(_) => 409,
                DomainError::Unauthorized(_) => 403,
                DomainError::Storage(_) => 500,
                DomainError::InvalidFieldValue { .. }
                | DomainError::UnknownLocale(_)
                | DomainError::UnknownAttribute(_)
                | DomainError::InvalidStateTransition { .. }
                | DomainError::InvalidSchemaDisplayField { .. }
                | DomainError::Validation(_) => 422,
            },
            Self::Unauthorized { .. } => 403,
            Self::NotFound { .. } => 404,
            Self::Validation(_) => 422,
            Self::Conflict { .. } => 409,
            Self::Internal(_) => 500,
        }
    }

    /// Converts this application error into an RFC 9457 compliant Problem Details representation.
    pub fn to_problem_details(&self) -> ProblemDetails {
        let status = self.status_code();
        let code = self.code();
        let title = match status {
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            409 => "Conflict",
            422 => "Unprocessable Entity",
            _ => "Internal Server Error",
        };

        let (detail, details, invalid_params) = match self {
            Self::Domain(d) => match d {
                DomainError::InvalidFieldValue {
                    attribute_id,
                    reason,
                } => (
                    d.to_string(),
                    Some(serde_json::json!({ "attribute_id": attribute_id.to_string() })),
                    Some(vec![InvalidParam {
                        name: attribute_id.to_string(),
                        reason: reason.clone(),
                        code: "invalid_value".to_string(),
                    }]),
                ),
                DomainError::UnknownAttribute(attr_id) => (
                    d.to_string(),
                    Some(serde_json::json!({ "attribute_id": attr_id.to_string() })),
                    None,
                ),
                DomainError::UnknownLocale(locale) => (
                    d.to_string(),
                    Some(serde_json::json!({ "locale": locale.to_string() })),
                    None,
                ),
                _ => (d.to_string(), None, None),
            },
            Self::Unauthorized { actor, action } => (
                format!("actor '{actor}' lacks permission for action '{action:?}'"),
                Some(serde_json::json!({
                    "actor": actor.to_string(),
                    "action": format!("{action:?}"),
                })),
                None,
            ),
            Self::NotFound { entity, id } => (
                format!("resource not found: {entity} with id '{id}'"),
                Some(serde_json::json!({
                    "entity": entity,
                    "id": id,
                })),
                None,
            ),
            Self::Validation(errors) => (
                errors.join("; "),
                None,
                Some(
                    errors
                        .iter()
                        .map(|e| InvalidParam {
                            name: "field".to_string(),
                            reason: e.clone(),
                            code: "validation_error".to_string(),
                        })
                        .collect(),
                ),
            ),
            Self::Conflict {
                message, details, ..
            } => (message.clone(), details.clone(), None),
            Self::Internal(msg) => (msg.clone(), None, None),
        };

        ProblemDetails {
            problem_type: format!("urn:luminair:error:{code}"),
            title: title.to_string(),
            status,
            code: code.to_string(),
            detail,
            details,
            invalid_params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::content::DocumentInstanceId;

    #[test]
    fn test_domain_error_conversion() {
        let id = DocumentInstanceId::new(uuid::Uuid::now_v7());
        let domain_err = DomainError::DocumentInstanceNotFound(id);
        let app_err: ApplicationError = domain_err.into();

        match &app_err {
            ApplicationError::Domain(DomainError::DocumentInstanceNotFound(found_id)) => {
                assert_eq!(*found_id, id);
            }
            _ => panic!("expected ApplicationError::Domain variant"),
        }
        assert!(app_err.to_string().contains("document instance not found"));
        assert_eq!(app_err.code(), "document_instance_not_found");
        assert_eq!(app_err.status_code(), 404);
    }

    #[test]
    fn test_unauthorized_error_display() {
        use domain::auth::UserId;

        let user_id = UserId::try_new("user-123").expect("valid user id");
        let err = ApplicationError::Unauthorized {
            actor: ActorId::User(user_id),
            action: Permission::PublishDocument(None),
        };

        let msg = err.to_string();
        assert!(msg.contains("actor 'user:user-123'"));
        assert!(msg.contains("PublishDocument"));
        assert_eq!(err.code(), "unauthorized");
        assert_eq!(err.status_code(), 403);
    }

    #[test]
    fn test_not_found_error_display() {
        let err = ApplicationError::NotFound {
            entity: "Role",
            id: "editor".to_string(),
        };

        assert_eq!(err.to_string(), "resource not found: Role with id 'editor'");
        assert_eq!(err.code(), "resource_not_found");
        assert_eq!(err.status_code(), 404);
    }

    #[test]
    fn test_validation_error_display_single() {
        let err = ApplicationError::Validation(vec!["title must not be empty".to_string()]);
        assert_eq!(err.to_string(), "validation error: title must not be empty");
        assert_eq!(err.code(), "validation_failed");
        assert_eq!(err.status_code(), 422);
    }

    #[test]
    fn test_validation_error_display_multiple() {
        let err = ApplicationError::Validation(vec![
            "title must not be empty".to_string(),
            "body locale 'fr' unknown".to_string(),
        ]);
        let msg = err.to_string();
        assert!(msg.contains("title must not be empty"));
        assert!(msg.contains("body locale 'fr' unknown"));
        assert_eq!(err.code(), "validation_failed");
        assert_eq!(err.status_code(), 422);
    }

    #[test]
    fn test_conflict_error_display_and_helpers() {
        let err = ApplicationError::conflict(
            ConflictCode::GenericConflict,
            "locale 'en' already active",
        );
        assert_eq!(
            err.to_string(),
            "conflict [generic_conflict]: locale 'en' already active"
        );
        assert_eq!(err.code(), "generic_conflict");
        assert_eq!(err.status_code(), 409);

        let doc_id = DocumentInstanceId::new(uuid::Uuid::now_v7());
        let overwrite_err = ApplicationError::draft_overwrite_conflict(doc_id, 1, 3);
        assert_eq!(overwrite_err.code(), "draft_has_pending_edits");
        let problem = overwrite_err.to_problem_details();
        assert_eq!(problem.status, 409);
        assert_eq!(problem.code, "draft_has_pending_edits");
        assert!(problem.details.is_some());
        let details = problem.details.unwrap();
        assert_eq!(details["document_id"], doc_id.to_string());
        assert_eq!(details["target_revision"], 1);
        assert_eq!(details["draft_version"], 3);
    }

    #[test]
    fn test_problem_details_serialization() {
        let doc_id = DocumentInstanceId::new(uuid::Uuid::now_v7());
        let err = ApplicationError::already_published_conflict(doc_id);
        let problem = err.to_problem_details();
        let json_str = serde_json::to_string(&problem).expect("serialize ProblemDetails");
        assert!(json_str.contains("\"code\":\"already_published\""));
        assert!(json_str.contains("\"status\":409"));
        assert!(json_str.contains("\"type\":\"urn:luminair:error:already_published\""));
        assert!(json_str.contains(&doc_id.to_string()));
    }

    #[test]
    fn test_internal_error_display() {
        let err = ApplicationError::Internal("failed to acquire lock".to_string());
        assert_eq!(err.to_string(), "internal error: failed to acquire lock");
        assert_eq!(err.code(), "internal_error");
        assert_eq!(err.status_code(), 500);
    }
}
