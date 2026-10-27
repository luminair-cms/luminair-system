//! Access request and user onboarding route handlers.

use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use uuid::Uuid;

use application::commands::{
    ApproveAccessRequestCommand, RejectAccessRequestCommand, SubmitAccessRequestCommand,
};
use application::services::AccessRequestsService;
use domain::auth::role::RoleId;

use crate::api::codec::{AccessRequestDto, RoleAssignmentDto};
use crate::api::envelope::{CreatedJson, JsonData};
use crate::api::error::ApiError;
use crate::api::extractors::parse_access_request_id;
use crate::api::state::{HasAccessRequestsService, HasAuthResolver};
use crate::auth::{Caller, OnboardingUser};

// ---------------------------------------------------------------------------
// Request Payload Structs (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Payload for approving an access request and assigning roles (`POST .../approve`).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveAccessRequestPayload {
    /// Role identifiers to grant the approved user.
    #[serde(alias = "role_ids")]
    pub role_ids: Vec<Uuid>,
}

/// Optional payload for rejecting an access request (`POST .../reject`).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectAccessRequestPayload {
    /// Optional administrative explanation for the rejection.
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Public Handler Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Submits an initial access request for an authenticated IdP identity (`POST /api/v1/auth/access-requests`).
pub async fn submit_access_request<St>(
    State(state): State<St>,
    OnboardingUser(user): OnboardingUser,
) -> Result<CreatedJson<AccessRequestDto>, ApiError>
where
    St: HasAccessRequestsService + HasAuthResolver,
{
    let cmd = SubmitAccessRequestCommand::new(user.user_id, user.email, user.name);
    let req = state.access_requests_service().submit(cmd).await?;
    Ok(CreatedJson::new(AccessRequestDto::from(req)))
}

/// Lists all pending access requests awaiting administrator review (`GET /api/v1/auth/access-requests`).
pub async fn list_pending_access_requests<St>(
    State(state): State<St>,
    Caller(caller): Caller,
) -> Result<JsonData<Vec<AccessRequestDto>>, ApiError>
where
    St: HasAccessRequestsService + HasAuthResolver,
{
    let reqs = state.access_requests_service().list_pending(&caller).await?;
    let dtos = reqs.into_iter().map(AccessRequestDto::from).collect();
    Ok(JsonData::new(dtos))
}

/// Fetches an access request by identifier (`GET /api/v1/auth/access-requests/{id}`).
pub async fn get_access_request_by_id<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path(raw_id): Path<String>,
) -> Result<JsonData<AccessRequestDto>, ApiError>
where
    St: HasAccessRequestsService + HasAuthResolver,
{
    let id = parse_access_request_id(&raw_id)?;
    let req = state.access_requests_service().get_by_id(&caller, id).await?;
    Ok(JsonData::new(AccessRequestDto::from(req)))
}

/// Approves an access request and assigns requested roles (`POST /api/v1/auth/access-requests/{id}/approve`).
pub async fn approve_access_request<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path(raw_id): Path<String>,
    Json(payload): Json<ApproveAccessRequestPayload>,
) -> Result<JsonData<Vec<RoleAssignmentDto>>, ApiError>
where
    St: HasAccessRequestsService + HasAuthResolver,
{
    let id = parse_access_request_id(&raw_id)?;
    let role_ids = payload.role_ids.into_iter().map(RoleId::new).collect();
    let cmd = ApproveAccessRequestCommand::new(id, role_ids);
    let assignments = state.access_requests_service().approve(&caller, cmd).await?;
    let dtos = assignments.into_iter().map(RoleAssignmentDto::from).collect();
    Ok(JsonData::new(dtos))
}

/// Rejects a pending access request with an optional reason (`POST /api/v1/auth/access-requests/{id}/reject`).
pub async fn reject_access_request<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Path(raw_id): Path<String>,
    payload: Option<Json<RejectAccessRequestPayload>>,
) -> Result<JsonData<AccessRequestDto>, ApiError>
where
    St: HasAccessRequestsService + HasAuthResolver,
{
    let id = parse_access_request_id(&raw_id)?;
    let reason = payload.and_then(|Json(p)| p.reason);
    let cmd = RejectAccessRequestCommand::new(id, reason);
    let req = state.access_requests_service().reject(&caller, cmd).await?;
    Ok(JsonData::new(AccessRequestDto::from(req)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use chrono::Utc;
    use domain::auth::access_request::{AccessRequest, AccessRequestId, AccessRequestStatus};
    use domain::auth::role::{UserRoleAssignment, UserRoleAssignmentId};
    use domain::auth::user::{ActorId, UserId};
    use domain::schema::SchemaRegistry;
    use domain::system::locale::LocaleId;
    use domain::system::{SystemConfig, SystemConfigId, SystemContext};

    use crate::api::state::HasSystemContext;
    use crate::auth::{AuthContextResolver, AuthResolutionError, AuthenticatedUser};

    #[derive(Clone)]
    struct MockAccessState {
        context: &'static SystemContext,
        resolver: Arc<MockAuthResolver>,
        service: Arc<MockAccessService>,
    }

    impl HasSystemContext for MockAccessState {
        fn system_context(&self) -> &'static SystemContext {
            self.context
        }
    }

    impl HasAuthResolver for MockAccessState {
        type Auth = MockAuthResolver;

        fn auth_resolver(&self) -> &Self::Auth {
            &self.resolver
        }
    }

    impl HasAccessRequestsService for MockAccessState {
        type AccessRequests = MockAccessService;

        fn access_requests_service(&self) -> &Self::AccessRequests {
            &self.service
        }
    }

    struct MockAuthResolver;

    impl AuthContextResolver for MockAuthResolver {
        async fn resolve_user_identity(
            &self,
            _auth_header: Option<&str>,
        ) -> Result<AuthenticatedUser, AuthResolutionError> {
            let user_id = UserId::try_new("usr_test").expect("valid user");
            Ok(AuthenticatedUser::new(user_id, None, None))
        }

        async fn resolve_caller(
            &self,
            _auth_header: Option<&str>,
        ) -> Result<application::context::CallerContext, AuthResolutionError> {
            let user_id = UserId::try_new("usr_test").expect("valid user");
            Ok(application::context::CallerContext::new(
                ActorId::User(user_id),
                vec![],
            ))
        }
    }

    struct MockAccessService {
        sample_request: AccessRequest,
    }

    impl AccessRequestsService for MockAccessService {
        async fn submit(
            &self,
            _cmd: SubmitAccessRequestCommand,
        ) -> Result<AccessRequest, application::errors::ApplicationError> {
            Ok(self.sample_request.clone())
        }

        async fn approve(
            &self,
            _caller: &application::context::CallerContext,
            cmd: ApproveAccessRequestCommand,
        ) -> Result<Vec<UserRoleAssignment>, application::errors::ApplicationError> {
            let user_id = self.sample_request.user_id.clone();
            let assignments = cmd
                .role_ids
                .into_iter()
                .map(|r| UserRoleAssignment {
                    id: UserRoleAssignmentId::new(Uuid::now_v7()),
                    user_id: user_id.clone(),
                    role_id: r,
                    granted_at: Utc::now(),
                    granted_by: Some(ActorId::User(user_id.clone())),
                })
                .collect();
            Ok(assignments)
        }

        async fn reject(
            &self,
            _caller: &application::context::CallerContext,
            cmd: RejectAccessRequestCommand,
        ) -> Result<AccessRequest, application::errors::ApplicationError> {
            let mut req = self.sample_request.clone();
            req.status = AccessRequestStatus::Rejected { reason: cmd.reason };
            Ok(req)
        }

        async fn get_by_id(
            &self,
            _caller: &application::context::CallerContext,
            _request_id: AccessRequestId,
        ) -> Result<AccessRequest, application::errors::ApplicationError> {
            Ok(self.sample_request.clone())
        }

        async fn list_pending(
            &self,
            _caller: &application::context::CallerContext,
        ) -> Result<Vec<AccessRequest>, application::errors::ApplicationError> {
            Ok(vec![self.sample_request.clone()])
        }
    }

    fn test_system_context() -> &'static SystemContext {
        let registry = SchemaRegistry::new(vec![]);
        let en = LocaleId::try_new("en").expect("valid locale");
        let config = SystemConfig::new(
            SystemConfigId::new(Uuid::now_v7()),
            vec![en.clone()],
            en,
        )
        .expect("valid config");
        Box::leak(Box::new(SystemContext::new(registry, config)))
    }

    fn sample_access_request() -> AccessRequest {
        AccessRequest {
            id: AccessRequestId::new(Uuid::now_v7()),
            user_id: UserId::try_new("usr_test").unwrap(),
            email: None,
            name: None,
            requested_at: Utc::now(),
            status: AccessRequestStatus::Pending,
            reviewed_by: None,
            reviewed_at: None,
            assigned_roles: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_submit_access_request_handler() {
        let req = sample_access_request();
        let state = MockAccessState {
            context: test_system_context(),
            resolver: Arc::new(MockAuthResolver),
            service: Arc::new(MockAccessService { sample_request: req }),
        };
        let user = OnboardingUser(AuthenticatedUser::new(
            UserId::try_new("usr_test").unwrap(),
            None,
            None,
        ));

        let res = submit_access_request(State(state), user)
            .await
            .expect("submission should succeed");
        assert_eq!(res.data.user_id.as_ref(), "usr_test");
    }

    #[tokio::test]
    async fn test_approve_access_request_handler() {
        let req = sample_access_request();
        let raw_id = req.id.to_string();
        let state = MockAccessState {
            context: test_system_context(),
            resolver: Arc::new(MockAuthResolver),
            service: Arc::new(MockAccessService { sample_request: req }),
        };
        let caller = Caller(application::context::CallerContext::new(
            ActorId::User(UserId::try_new("usr_admin").unwrap()),
            vec![],
        ));
        let payload = ApproveAccessRequestPayload {
            role_ids: vec![Uuid::now_v7()],
        };

        let res = approve_access_request(State(state), caller, Path(raw_id), Json(payload))
            .await
            .expect("approval should succeed");
        assert_eq!(res.data.len(), 1);
    }
}
