# UD — User Directory Specification

- **Prefix**: `UD`
- **Layer**: `application`
- **Unit under test**: `application::services::UserDirectoryService`
- **Format**: `U` (use case rules)
- **Test target**: `application/tests/ud_user_directory_service.rs`

---

## Vocabulary

| Term | Meaning / API symbol | Concrete representation |
|---|---|---|
| `UserProfile` | Domain identity profile | `domain::auth::UserProfile` |
| `UserDirectoryService` | Directory query and profile sync service | `application::services::UserDirectoryService` |
| `ManageUsers` | Permission required for directory listing | `Permission::ManageUsers` |
| `SelfLookup` | Requester inspecting their own profile | `caller.user_id() == Some(target_id)` |

---

## Rules

### UD-01 — List Users Requires ManageUsers Permission
- **Status**: `Approved`
- **Origin**: `adr:006`
- **Given**: A caller without `Permission::ManageUsers` vs a caller with `Permission::ManageUsers` or `Admin`.
- **When**: Caller invokes `service.list_users(caller, cmd).await`.
- **Then**: Unprivileged caller receives `Err(ApplicationError::Unauthorized)`, while authorized caller receives `Ok(page)`.

### UD-02 — List Users Filters By Name And Email And Supports Pagination
- **Status**: `Approved`
- **Origin**: `adr:006`
- **Given**: Multiple user profiles registered in the directory.
- **When**: Calling `list_users` with a search query string and pagination parameters.
- **Then**: Returns matching profiles matching query against email, name, or subject identifier, sorted with correct `total` count and paginated items.

### UD-03 — Get User Allows Self Lookup Without ManageUsers Permission
- **Status**: `Approved`
- **Origin**: `adr:006`
- **Given**: A user profile for `user_id`, and a caller whose identity is `user_id` with no roles.
- **When**: Caller invokes `service.get_user(caller, &user_id).await`.
- **Then**: Returns `Ok(Some(profile))` containing the user's profile.

### UD-04 — Get User Denies Other User Without ManageUsers Permission
- **Status**: `Approved`
- **Origin**: `adr:006`
- **Given**: A user profile for `bob`, and a caller whose identity is `alice` with no administrative roles.
- **When**: Caller invokes `service.get_user(caller, &bob_id).await`.
- **Then**: Returns `Err(ApplicationError::Unauthorized)`.

### UD-05 — Get User Allows Admin To Lookup Any User
- **Status**: `Approved`
- **Origin**: `adr:006`
- **Given**: A user profile for `bob`, and a caller possessing `Permission::ManageUsers`.
- **When**: Caller invokes `service.get_user(caller, &bob_id).await`.
- **Then**: Returns `Ok(Some(profile))` containing bob's profile.

### UD-06 — Sync Profile Creates New User Profile
- **Status**: `Approved`
- **Origin**: `adr:006`
- **Given**: No existing user profile for `user_id`.
- **When**: Invoking `service.sync_profile(SyncProfileCommand::new(user_id, email, name)).await`.
- **Then**: Persists and returns a new `UserProfile` with `first_seen_at == last_seen_at`.

### UD-07 — Sync Profile Updates Existing Profile And Preserves First Seen At
- **Status**: `Approved`
- **Origin**: `adr:006`
- **Given**: An existing user profile with initial claims.
- **When**: Invoking `service.sync_profile` with updated email and name claims.
- **Then**: Updates mutable claims, advances `last_seen_at`, preserves `first_seen_at`, and persists to repository.
