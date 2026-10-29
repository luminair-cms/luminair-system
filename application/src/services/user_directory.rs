//! User directory application service.

use std::future::Future;

use chrono::Utc;
use domain::auth::{UserId, UserProfile, UserProfileRepository};
use domain::common::Page;

use crate::commands::user_directory::{ListUsersCommand, SyncProfileCommand};
use crate::context::CallerContext;
use crate::errors::ApplicationError;

// ---------------------------------------------------------------------------
// Port Trait
// ---------------------------------------------------------------------------

/// Port trait defining use cases for user identity profiles and directory search.
pub trait UserDirectoryService: Send + Sync + 'static {
    /// Lists user profiles matching an optional search query with pagination.
    ///
    /// Requires [`Permission::ManageUsers`].
    ///
    /// # Errors
    /// Returns [`ApplicationError::Unauthorized`] if the caller lacks permission.
    /// Returns [`ApplicationError::Domain`] if the underlying repository fails.
    fn list_users(
        &self,
        caller: &CallerContext,
        cmd: ListUsersCommand,
    ) -> impl Future<Output = Result<Page<UserProfile>, ApplicationError>> + Send;

    /// Fetches a user profile by subject identifier (`sub`).
    ///
    /// Allowed for the user themselves (`caller.user_id() == Some(user_id)`)
    /// or callers with [`Permission::ManageUsers`].
    ///
    /// # Errors
    /// Returns [`ApplicationError::Unauthorized`] if the caller lacks permission.
    /// Returns [`ApplicationError::Domain`] if the underlying repository fails.
    fn get_user(
        &self,
        caller: &CallerContext,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<UserProfile>, ApplicationError>> + Send;

    /// Synchronizes external IdP claims into the local profile repository.
    ///
    /// If the profile already exists, updates mutable claims (`email`, `name`) and touches `last_seen_at`.
    /// If absent, registers a new profile.
    ///
    /// # Errors
    /// Returns [`ApplicationError::Domain`] if the underlying repository fails.
    fn sync_profile(
        &self,
        cmd: SyncProfileCommand,
    ) -> impl Future<Output = Result<UserProfile, ApplicationError>> + Send;
}

// ---------------------------------------------------------------------------
// Service Implementation
// ---------------------------------------------------------------------------

/// Default implementation of [`UserDirectoryService`] generic over repository storage.
#[derive(Debug, Clone)]
pub struct UserDirectoryServiceImpl<U> {
    pub repo: U,
}

impl<U> UserDirectoryServiceImpl<U>
where
    U: UserProfileRepository + 'static,
{
    /// Creates a new `UserDirectoryServiceImpl`.
    #[must_use]
    pub fn new(repo: U) -> Self {
        Self { repo }
    }
}

impl<U> UserDirectoryService for UserDirectoryServiceImpl<U>
where
    U: UserProfileRepository + 'static,
{
    async fn list_users(
        &self,
        caller: &CallerContext,
        cmd: ListUsersCommand,
    ) -> Result<Page<UserProfile>, ApplicationError> {
        caller.check_manage_users()?;
        let page = self
            .repo
            .list_users(cmd.query.as_deref(), cmd.pagination)
            .await?;
        Ok(page)
    }

    async fn get_user(
        &self,
        caller: &CallerContext,
        user_id: &UserId,
    ) -> Result<Option<UserProfile>, ApplicationError> {
        let is_self = caller.user_id() == Some(user_id);
        if !is_self {
            caller.check_manage_users()?;
        }
        let profile = self.repo.find_by_id(user_id).await?;
        Ok(profile)
    }

    async fn sync_profile(
        &self,
        cmd: SyncProfileCommand,
    ) -> Result<UserProfile, ApplicationError> {
        let now = Utc::now();
        let existing = self.repo.find_by_id(&cmd.user_id).await?;
        let profile = match existing {
            Some(mut profile) => {
                profile.sync_claims(cmd.email, cmd.name, now);
                self.repo.save(&profile).await?;
                profile
            }
            None => {
                let profile = UserProfile::new(cmd.user_id, cmd.email, cmd.name, now);
                self.repo.save(&profile).await?;
                profile
            }
        };
        Ok(profile)
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::auth::Permission;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    use domain::auth::role::Role;
    use domain::common::{DisplayName, Email, Pagination};
    use domain::errors::DomainError;
    use uuid::Uuid;

    #[derive(Debug, Clone, Default)]
    struct MockProfileRepo {
        profiles: Arc<RwLock<HashMap<UserId, UserProfile>>>,
    }

    impl UserProfileRepository for MockProfileRepo {
        async fn find_by_id(&self, user_id: &UserId) -> Result<Option<UserProfile>, DomainError> {
            let store = self.profiles.read().unwrap();
            Ok(store.get(user_id).cloned())
        }

        async fn find_by_ids(&self, user_ids: &[UserId]) -> Result<Vec<UserProfile>, DomainError> {
            let store = self.profiles.read().unwrap();
            Ok(user_ids.iter().filter_map(|id| store.get(id).cloned()).collect())
        }

        async fn list_users(
            &self,
            query: Option<&str>,
            pagination: Pagination,
        ) -> Result<Page<UserProfile>, DomainError> {
            let store = self.profiles.read().unwrap();
            let mut items: Vec<UserProfile> = store
                .values()
                .filter(|p| {
                    if let Some(q) = query {
                        let q = q.to_lowercase();
                        p.user_id.as_str().to_lowercase().contains(&q)
                            || p.email
                                .as_ref()
                                .map(|e| e.as_str().to_lowercase().contains(&q))
                                .unwrap_or(false)
                            || p.name
                                .as_ref()
                                .map(|n| n.as_str().to_lowercase().contains(&q))
                                .unwrap_or(false)
                    } else {
                        true
                    }
                })
                .cloned()
                .collect();
            items.sort_by(|a, b| a.user_id.as_str().cmp(b.user_id.as_str()));
            let total = items.len() as u64;
            Ok(Page::new(items, total, pagination.page, pagination.page_size))
        }

        async fn save(&self, profile: &UserProfile) -> Result<(), DomainError> {
            let mut store = self.profiles.write().unwrap();
            store.insert(profile.user_id.clone(), profile.clone());
            Ok(())
        }
    }

    fn user_caller(user_name: &str, roles: Vec<Role>) -> CallerContext {
        let user_id = UserId::try_new(user_name).unwrap();
        CallerContext::new(user_id, roles)
    }

    fn admin_role() -> Role {
        Role {
            id: domain::auth::RoleId::new(Uuid::now_v7()),
            name: "admin".into(),
            description: None,
            permissions: vec![Permission::ManageUsers],
        }
    }

    #[tokio::test]
    async fn test_list_users_denied_without_manage_users() {
        let repo = MockProfileRepo::default();
        let service = UserDirectoryServiceImpl::new(repo);
        let caller = user_caller("alice", vec![]);

        let result = service.list_users(&caller, ListUsersCommand::default()).await;
        assert!(matches!(result, Err(ApplicationError::Unauthorized { .. })));
    }

    #[tokio::test]
    async fn test_list_users_allowed_with_manage_users() {
        let repo = MockProfileRepo::default();
        let service = UserDirectoryServiceImpl::new(repo);
        let caller = user_caller("admin_user", vec![admin_role()]);

        let result = service.list_users(&caller, ListUsersCommand::default()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_user_allowed_for_self_without_roles() {
        let user_id = UserId::try_new("alice").unwrap();
        let repo = MockProfileRepo::default();
        let caller = user_caller("alice", vec![]);
        let service = UserDirectoryServiceImpl::new(repo);

        let result = service.get_user(&caller, &user_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_user_denied_for_other_user_without_roles() {
        let target_user = UserId::try_new("bob").unwrap();
        let repo = MockProfileRepo::default();
        let caller = user_caller("alice", vec![]);
        let service = UserDirectoryServiceImpl::new(repo);

        let result = service.get_user(&caller, &target_user).await;
        assert!(matches!(result, Err(ApplicationError::Unauthorized { .. })));
    }

    #[tokio::test]
    async fn test_get_user_allowed_for_other_user_with_manage_users() {
        let target_user = UserId::try_new("bob").unwrap();
        let repo = MockProfileRepo::default();
        let caller = user_caller("admin_user", vec![admin_role()]);
        let service = UserDirectoryServiceImpl::new(repo);

        let result = service.get_user(&caller, &target_user).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sync_profile_creates_and_updates() {
        let repo = MockProfileRepo::default();
        let service = UserDirectoryServiceImpl::new(repo);

        let user_id = UserId::try_new("carol").unwrap();
        let email1 = Email::try_new("carol@example.com").ok();
        let name1 = DisplayName::try_new("Carol A").ok();

        // 1. Initial sync creates new profile
        let profile1 = service
            .sync_profile(SyncProfileCommand::new(user_id.clone(), email1, name1))
            .await
            .expect("initial sync succeeds");

        assert_eq!(profile1.user_id, user_id);
        assert_eq!(profile1.email.as_ref().map(|e| e.as_str()), Some("carol@example.com"));
        assert_eq!(profile1.name.as_ref().map(|n| n.as_str()), Some("Carol A"));

        // 2. Subsequent sync updates profile claims
        let email2 = Email::try_new("carol.updated@example.com").ok();
        let name2 = DisplayName::try_new("Carol B").ok();

        let profile2 = service
            .sync_profile(SyncProfileCommand::new(user_id.clone(), email2, name2))
            .await
            .expect("second sync succeeds");

        assert_eq!(profile2.user_id, user_id);
        assert_eq!(profile2.email.as_ref().map(|e| e.as_str()), Some("carol.updated@example.com"));
        assert_eq!(profile2.name.as_ref().map(|n| n.as_str()), Some("Carol B"));
        assert_eq!(profile2.first_seen_at, profile1.first_seen_at);
        assert!(profile2.last_seen_at >= profile1.last_seen_at);
    }
}
