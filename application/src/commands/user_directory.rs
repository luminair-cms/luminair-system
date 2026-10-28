//! Commands for user profile and directory operations.

use domain::auth::UserId;
use domain::common::{DisplayName, Email, Pagination};

/// Command to search and list user profiles with pagination.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListUsersCommand {
    /// Optional case-insensitive search string matching email or name.
    pub query: Option<String>,
    /// Pagination specification (page and page size).
    pub pagination: Pagination,
}

/// Command to synchronize external IdP claims into local storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncProfileCommand {
    /// Subject identifier (`sub`) issued by the external IdP.
    pub user_id: UserId,
    /// Verified email claim from the IdP token.
    pub email: Option<Email>,
    /// Display name claim from the IdP token.
    pub name: Option<DisplayName>,
}

impl ListUsersCommand {
    /// Creates a new `ListUsersCommand`.
    #[must_use]
    pub fn new(query: Option<String>, pagination: Pagination) -> Self {
        Self { query, pagination }
    }
}

impl SyncProfileCommand {
    /// Creates a new `SyncProfileCommand`.
    #[must_use]
    pub fn new(user_id: UserId, email: Option<Email>, name: Option<DisplayName>) -> Self {
        Self {
            user_id,
            email,
            name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_users_command_construction_and_defaults() {
        let default_cmd = ListUsersCommand::default();
        assert_eq!(default_cmd.query, None);
        assert_eq!(default_cmd.pagination, Pagination::default());

        let custom_cmd = ListUsersCommand::new(
            Some("alice".into()),
            Pagination::new(2, 50),
        );
        assert_eq!(custom_cmd.query.as_deref(), Some("alice"));
        assert_eq!(custom_cmd.pagination.page, 2);
        assert_eq!(custom_cmd.pagination.page_size, 50);
    }

    #[test]
    fn test_sync_profile_command_construction() {
        let user_id = UserId::try_new("user_abc").expect("valid user id");
        let email = Email::try_new("alice@example.com").ok();
        let name = DisplayName::try_new("Alice").ok();

        let cmd = SyncProfileCommand::new(user_id.clone(), email.clone(), name.clone());
        assert_eq!(cmd.user_id, user_id);
        assert_eq!(cmd.email, email);
        assert_eq!(cmd.name, name);
    }
}
