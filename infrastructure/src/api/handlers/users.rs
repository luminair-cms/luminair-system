//! User directory and profile management route handlers.

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use application::commands::user_directory::{ListUsersCommand, SyncProfileCommand};
use application::services::UserDirectoryService;
use domain::common::{DisplayName, Email};

use crate::api::codec::UserProfileDto;
use crate::api::envelope::{JsonData, PagedJson};
use crate::api::error::ApiError;
use crate::api::extractors::ListUsersQuery;
use crate::api::state::{HasAuthResolver, HasUserDirectoryService};
use crate::auth::{Caller, OnboardingUser};

// ---------------------------------------------------------------------------
// Request Payload Types (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Optional request payload for profile synchronization (`POST /api/v1/auth/profile/sync`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncProfilePayload {
    /// Overriding or complementary email address.
    pub email: Option<String>,
    /// Overriding or complementary display name.
    pub name: Option<String>,
}

// ---------------------------------------------------------------------------
// Public Handler Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

/// Lists user profiles matching an optional search string (`GET /api/v1/auth/users`).
pub async fn list_users<St>(
    State(state): State<St>,
    Caller(caller): Caller,
    Query(query): Query<ListUsersQuery>,
) -> Result<PagedJson<UserProfileDto>, ApiError>
where
    St: HasUserDirectoryService + HasAuthResolver,
{
    let pagination = query.pagination();
    let cmd = ListUsersCommand::new(query.q, pagination);
    let page = state.user_directory_service().list_users(&caller, cmd).await?;
    let dtos: Vec<UserProfileDto> = page.items.into_iter().map(UserProfileDto::from).collect();
    Ok(PagedJson::new(dtos, pagination, page.total))
}

/// Synchronizes authenticated IdP claims into local user profile storage (`POST /api/v1/auth/profile/sync`).
pub async fn sync_profile<St>(
    State(state): State<St>,
    OnboardingUser(user): OnboardingUser,
    payload: Option<Json<SyncProfilePayload>>,
) -> Result<JsonData<UserProfileDto>, ApiError>
where
    St: HasUserDirectoryService + HasAuthResolver,
{
    let payload = payload.map(|Json(p)| p).unwrap_or_default();
    let email = payload
        .email
        .as_deref()
        .map(Email::try_new)
        .transpose()
        .map_err(|e| ApiError::bad_request("invalid_email", e.to_string()))?
        .or(user.email);
    let name = payload
        .name
        .as_deref()
        .map(DisplayName::try_new)
        .transpose()
        .map_err(|e| ApiError::bad_request("invalid_name", e.to_string()))?
        .or(user.name);

    let cmd = SyncProfileCommand::new(user.user_id, email, name);
    let profile = state.user_directory_service().sync_profile(cmd).await?;
    Ok(JsonData::new(UserProfileDto::from(profile)))
}
