//! Application-level error types and RFC 9457 Problem Details mapping.

use std::fmt;
use std::ops::Deref;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use domain::auth::{ActorId, Permission, UserId};
use domain::content::DocumentInstanceId;
use domain::errors::DomainError;

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

/// Status of an authenticated user's onboarding access request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnboardingStatus {
    /// User has not yet submitted an access request.
    AccessNotRequested { user_id: UserId },
    /// User has submitted an access request that is currently pending review.
    AccessPending {
        user_id: UserId,
        submitted_at: DateTime<Utc>,
    },
    /// User's access request was rejected.
    AccessRejected {
        user_id: UserId,
        reason: Option<String>,
    },
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
    /// Stable, machine-readable application error code (e.g. "`draft_has_pending_edits`").
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

    /// Authenticated user cannot access studio resources due to onboarding status.
    #[error("onboarding access denied: {0:?}")]
    Onboarding(OnboardingStatus),

    /// A requested resource was not found.
    #[error("resource not found: {entity} with id '{id}'")]
    NotFound {
        /// Name of the entity type that could not be found.
        entity: &'static str,
        /// The identifier that was looked up.
        id: String,
    },

    /// An input command or query failed application-level validation.
    /// Contains field-level validation errors — never truncated to the first error.
    #[error("validation error: {}", format_invalid_params(.0))]
    Validation(Vec<InvalidParam>),

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

impl ConflictCode {
    /// Returns the static `snake_case` string representation of the conflict code.
    #[must_use]
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

impl InvalidParam {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        reason: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            reason: reason.into(),
            code: code.into(),
        }
    }

    /// Converts a `DomainError` into a structured `InvalidParam` if applicable.
    #[must_use]
    pub fn from_domain_error(err: &DomainError) -> Self {
        match err {
            DomainError::InvalidFieldValue {
                attribute_id,
                reason,
                ..
            } => Self::new(
                attribute_id.to_string(),
                reason.clone(),
                "invalid_field_value",
            ),
            DomainError::UnknownAttribute(attr_id) => Self::new(
                attr_id.to_string(),
                format!("attribute '{attr_id}' is not declared in schema"),
                "unknown_attribute",
            ),
            DomainError::UnknownLocale(locale) => Self::new(
                "locale",
                format!("locale '{locale}' is not configured in system locales"),
                "unknown_locale",
            ),
            DomainError::Validation(msg) => {
                if msg.contains("required") {
                    Self::new("field", msg.clone(), "required_field_missing")
                } else {
                    Self::new("field", msg.clone(), "validation_error")
                }
            }
            other => Self::new("field", other.to_string(), "validation_error"),
        }
    }
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
    #[must_use]
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
    #[must_use]
    pub fn already_published_conflict(document_id: DocumentInstanceId) -> Self {
        Self::Conflict {
            code: ConflictCode::AlreadyPublished,
            message: format!(
                "document '{document_id}' is already published with no pending changes"
            ),
            details: Some(serde_json::json!({
                "document_id": document_id.to_string(),
            })),
        }
    }

    /// Convenience constructor for publish/unpublish when `draft_and_publish` is disabled.
    #[must_use]
    pub fn draft_publish_disabled_conflict(type_title: &str) -> Self {
        Self::Conflict {
            code: ConflictCode::DraftAndPublishDisabled,
            message: format!(
                "document type '{type_title}' does not support draft-and-publish workflow"
            ),
            details: Some(serde_json::json!({
                "document_type": type_title,
            })),
        }
    }

    /// Convenience constructor for optimistic concurrency control version mismatch.
    #[must_use]
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

    /// Convenience constructor for user who has not requested access.
    #[must_use]
    pub fn access_not_requested(user_id: UserId) -> Self {
        Self::Onboarding(OnboardingStatus::AccessNotRequested { user_id })
    }

    /// Convenience constructor for user whose access request is pending review.
    #[must_use]
    pub fn access_pending(user_id: UserId, submitted_at: DateTime<Utc>) -> Self {
        Self::Onboarding(OnboardingStatus::AccessPending {
            user_id,
            submitted_at,
        })
    }

    /// Convenience constructor for user whose access request was rejected.
    #[must_use]
    pub fn access_rejected(user_id: UserId, reason: Option<String>) -> Self {
        Self::Onboarding(OnboardingStatus::AccessRejected { user_id, reason })
    }

    /// Convenience constructor for single validation failure.
    #[must_use]
    pub fn validation_single(
        name: impl Into<String>,
        reason: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        Self::Validation(vec![InvalidParam::new(name, reason, code)])
    }

    /// Returns the canonical machine-readable `snake_case` error code.
    #[must_use]
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
            Self::Onboarding(status) => match status {
                OnboardingStatus::AccessNotRequested { .. } => "access_not_requested",
                OnboardingStatus::AccessPending { .. } => "access_pending",
                OnboardingStatus::AccessRejected { .. } => "access_rejected",
            },
            Self::NotFound { .. } => "resource_not_found",
            Self::Validation(_) => "validation_failed",
            Self::Conflict { code, .. } => code.as_str(),
            Self::Internal(_) => "internal_error",
        }
    }

    /// Returns the HTTP status code recommended for this error.
    #[must_use]
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
            Self::Unauthorized { .. } | Self::Onboarding(_) => 403,
            Self::NotFound { .. } => 404,
            Self::Validation(_) => 422,
            Self::Conflict { .. } => 409,
            Self::Internal(_) => 500,
        }
    }

    /// Converts this application error into an RFC 9457 compliant Problem Details representation.
    #[must_use]
    pub fn to_problem_details(&self) -> ProblemDetails {
        let status = self.status_code();
        let code = self.code();
        let (detail, details, invalid_params) = self.problem_details_payload();

        ProblemDetails {
            problem_type: format!("urn:luminair:error:{code}"),
            title: status_title(status).to_string(),
            status,
            code: code.to_string(),
            detail,
            details,
            invalid_params,
        }
    }

    fn problem_details_payload(
        &self,
    ) -> (String, Option<serde_json::Value>, Option<Vec<InvalidParam>>) {
        match self {
            Self::Domain(d) => match d {
                DomainError::InvalidFieldValue {
                    attribute_id,
                    reason,
                    ..
                } => (
                    d.to_string(),
                    Some(serde_json::json!({ "attribute_id": attribute_id.to_string() })),
                    Some(vec![InvalidParam {
                        name: attribute_id.to_string(),
                        reason: reason.clone(),
                        code: "invalid_field_value".to_string(),
                    }]),
                ),
                DomainError::UnknownAttribute(attr_id) => (
                    d.to_string(),
                    Some(serde_json::json!({ "attribute_id": attr_id.to_string() })),
                    Some(vec![InvalidParam {
                        name: attr_id.to_string(),
                        reason: format!("attribute '{attr_id}' is not declared in schema"),
                        code: "unknown_attribute".to_string(),
                    }]),
                ),
                DomainError::UnknownLocale(locale) => (
                    d.to_string(),
                    Some(serde_json::json!({ "locale": locale.to_string() })),
                    Some(vec![InvalidParam {
                        name: "locale".to_string(),
                        reason: format!("locale '{locale}' is not configured in system locales"),
                        code: "unknown_locale".to_string(),
                    }]),
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
            Self::Onboarding(status) => match status {
                OnboardingStatus::AccessNotRequested { user_id } => (
                    format!("user '{user_id}' has not submitted an access request"),
                    Some(serde_json::json!({ "user_id": user_id.to_string() })),
                    None,
                ),
                OnboardingStatus::AccessPending {
                    user_id,
                    submitted_at,
                } => (
                    format!("user '{user_id}' access request is pending review"),
                    Some(serde_json::json!({
                        "user_id": user_id.to_string(),
                        "submitted_at": submitted_at.to_rfc3339(),
                    })),
                    None,
                ),
                OnboardingStatus::AccessRejected { user_id, reason } => (
                    format!(
                        "user '{user_id}' access request was rejected: {}",
                        reason.as_deref().unwrap_or("no reason provided")
                    ),
                    Some(serde_json::json!({
                        "user_id": user_id.to_string(),
                        "reason": reason,
                    })),
                    None,
                ),
            },
            Self::NotFound { entity, id } => (
                format!("resource not found: {entity} with id '{id}'"),
                Some(serde_json::json!({
                    "entity": entity,
                    "id": id,
                })),
                None,
            ),
            Self::Validation(params) => (format_invalid_params(params), None, Some(params.clone())),
            Self::Conflict {
                message, details, ..
            } => (message.clone(), details.clone(), None),
            Self::Internal(msg) => (msg.clone(), None, None),
        }
    }
}

// ---------------------------------------------------------------------------
// Trait Implementations
// ---------------------------------------------------------------------------

impl fmt::Display for ConflictCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for InvalidParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.reason)
    }
}

impl Deref for InvalidParam {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.reason
    }
}

// ---------------------------------------------------------------------------
// Helper Functions
// ---------------------------------------------------------------------------

fn format_invalid_params(params: &[InvalidParam]) -> String {
    params
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

const fn status_title(status: u16) -> &'static str {
    match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        _ => "Internal Server Error",
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
        let err = ApplicationError::validation_single(
            "title",
            "title must not be empty",
            "required_field_missing",
        );
        assert_eq!(
            err.to_string(),
            "validation error: title: title must not be empty"
        );
        assert_eq!(err.code(), "validation_failed");
        assert_eq!(err.status_code(), 422);

        let problem = err.to_problem_details();
        assert_eq!(problem.invalid_params.as_ref().unwrap().len(), 1);
        assert_eq!(
            problem.invalid_params.as_ref().unwrap()[0].code,
            "required_field_missing"
        );
    }

    #[test]
    fn test_validation_error_display_multiple() {
        let err = ApplicationError::Validation(vec![
            InvalidParam::new("title", "title must not be empty", "required_field_missing"),
            InvalidParam::new("body", "body locale 'fr' unknown", "unknown_locale"),
        ]);
        let msg = err.to_string();
        assert!(msg.contains("title: title must not be empty"));
        assert!(msg.contains("body: body locale 'fr' unknown"));
        assert_eq!(err.code(), "validation_failed");
        assert_eq!(err.status_code(), 422);

        let problem = err.to_problem_details();
        let params = problem.invalid_params.unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "title");
        assert_eq!(params[1].name, "body");
    }

    #[test]
    fn test_onboarding_errors() {
        let user = UserId::try_new("alice").unwrap();
        let err_not_requested = ApplicationError::access_not_requested(user.clone());
        assert_eq!(err_not_requested.code(), "access_not_requested");
        assert_eq!(err_not_requested.status_code(), 403);
        let p1 = err_not_requested.to_problem_details();
        assert_eq!(p1.details.unwrap()["user_id"], "alice");

        let err_pending = ApplicationError::access_pending(user.clone(), Utc::now());
        assert_eq!(err_pending.code(), "access_pending");
        assert_eq!(err_pending.status_code(), 403);

        let err_rejected =
            ApplicationError::access_rejected(user, Some("Incomplete application".into()));
        assert_eq!(err_rejected.code(), "access_rejected");
        assert_eq!(err_rejected.status_code(), 403);
        let p3 = err_rejected.to_problem_details();
        assert_eq!(p3.details.unwrap()["reason"], "Incomplete application");
    }

    #[test]
    fn test_conflict_error_display_and_helpers() {
        let err =
            ApplicationError::conflict(ConflictCode::GenericConflict, "locale 'en' already active");
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
