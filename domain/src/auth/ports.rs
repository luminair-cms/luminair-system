use std::future::Future;

use super::access_request::{AccessRequest, AccessRequestId};
use super::profile::UserProfile;
use super::role::{Role, RoleId, UserRoleAssignment, UserRoleAssignmentId};
use super::user::UserId;
use crate::common::{Page, Pagination};
use crate::errors::DomainError;

pub trait RoleRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: RoleId,
    ) -> impl Future<Output = Result<Option<Role>, DomainError>> + Send;

    fn find_by_ids(
        &self,
        ids: &[RoleId],
    ) -> impl Future<Output = Result<Vec<Role>, DomainError>> + Send;

    fn find_by_name(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Option<Role>, DomainError>> + Send;

    fn find_all(&self) -> impl Future<Output = Result<Vec<Role>, DomainError>> + Send;

    fn save(&self, role: &Role) -> impl Future<Output = Result<(), DomainError>> + Send;
}

pub trait UserRoleAssignmentRepository: Send + Sync {
    fn find_by_user(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Vec<UserRoleAssignment>, DomainError>> + Send;

    fn exists_admin(
        &self,
        admin_role_id: RoleId,
    ) -> impl Future<Output = Result<bool, DomainError>> + Send;

    fn save(
        &self,
        assignment: &UserRoleAssignment,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    fn save_all(
        &self,
        assignments: &[UserRoleAssignment],
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    fn delete(
        &self,
        id: UserRoleAssignmentId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;
}

pub trait AccessRequestRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: AccessRequestId,
    ) -> impl Future<Output = Result<Option<AccessRequest>, DomainError>> + Send;

    fn find_by_user(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<AccessRequest>, DomainError>> + Send;

    fn find_active_by_user(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<AccessRequest>, DomainError>> + Send;

    fn find_pending(&self) -> impl Future<Output = Result<Vec<AccessRequest>, DomainError>> + Send;

    fn save(&self, request: &AccessRequest)
    -> impl Future<Output = Result<(), DomainError>> + Send;
}

pub trait UserProfileRepository: Send + Sync {
    /// Loads a user profile by immutable subject ID (`sub`).
    fn find_by_id(
        &self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<UserProfile>, DomainError>> + Send;

    /// Batch-loads user profiles by immutable subject IDs.
    fn find_by_ids(
        &self,
        user_ids: &[UserId],
    ) -> impl Future<Output = Result<Vec<UserProfile>, DomainError>> + Send;

    /// Paginated search over user profiles matching query against `email` or `name`.
    fn list_users(
        &self,
        query: Option<&str>,
        pagination: Pagination,
    ) -> impl Future<Output = Result<Page<UserProfile>, DomainError>> + Send;

    /// Persists or updates a user profile.
    fn save(&self, profile: &UserProfile) -> impl Future<Output = Result<(), DomainError>> + Send;
}
