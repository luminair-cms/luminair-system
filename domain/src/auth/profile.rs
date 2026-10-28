use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::user::UserId;
use crate::common::{DisplayName, Email};

// ---------------------------------------------------------------------------
// Entity Definition
// ---------------------------------------------------------------------------

/// Local user identity profile representing an authenticated external user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserProfile {
    /// Immutable Subject Identifier (`sub`) issued by the external IdP.
    pub user_id: UserId,
    /// Current verified email address.
    pub email: Option<Email>,
    /// Current display name.
    pub name: Option<DisplayName>,
    /// Timestamp when this identity was first encountered.
    pub first_seen_at: DateTime<Utc>,
    /// Timestamp of the most recent recorded authentication or activity.
    pub last_seen_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl UserProfile {
    /// Constructs a new user profile upon first encounter.
    #[must_use]
    pub fn new(
        user_id: UserId,
        email: Option<Email>,
        name: Option<DisplayName>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            user_id,
            email,
            name,
            first_seen_at: now,
            last_seen_at: now,
        }
    }

    /// Reconstitutes an existing user profile from persistent storage without triggering mutations.
    #[must_use]
    pub fn reconstitute(
        user_id: UserId,
        email: Option<Email>,
        name: Option<DisplayName>,
        first_seen_at: DateTime<Utc>,
        last_seen_at: DateTime<Utc>,
    ) -> Self {
        Self {
            user_id,
            email,
            name,
            first_seen_at,
            last_seen_at,
        }
    }

    /// Updates mutable profile claims from an incoming IdP token.
    ///
    /// Updates `last_seen_at` to `now`. Returns `true` if `email` or `name` changed, `false` otherwise.
    pub fn sync_claims(
        &mut self,
        email: Option<Email>,
        name: Option<DisplayName>,
        now: DateTime<Utc>,
    ) -> bool {
        let changed = self.email != email || self.name != name;
        if changed {
            self.email = email;
            self.name = name;
        }
        self.last_seen_at = now;
        changed
    }

    /// Touches the profile to record activity, advancing `last_seen_at`.
    pub fn touch(&mut self, now: DateTime<Utc>) {
        self.last_seen_at = now;
    }
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn test_user_id() -> UserId {
        UserId::try_new("user_12345").expect("valid user id")
    }

    fn test_email(raw: &str) -> Email {
        Email::try_new(raw).expect("valid email")
    }

    fn test_name(raw: &str) -> DisplayName {
        DisplayName::try_new(raw).expect("valid display name")
    }

    #[test]
    fn test_new_profile_initializes_timestamps_and_fields() {
        let now = Utc::now();
        let user_id = test_user_id();
        let email = Some(test_email("alice@example.com"));
        let name = Some(test_name("Alice Smith"));

        let profile = UserProfile::new(user_id.clone(), email.clone(), name.clone(), now);

        assert_eq!(profile.user_id, user_id);
        assert_eq!(profile.email, email);
        assert_eq!(profile.name, name);
        assert_eq!(profile.first_seen_at, now);
        assert_eq!(profile.last_seen_at, now);
    }

    #[test]
    fn test_reconstitute_preserves_distinct_timestamps() {
        let first_seen = Utc::now() - Duration::days(30);
        let last_seen = Utc::now();
        let user_id = test_user_id();
        let email = Some(test_email("bob@example.com"));
        let name = Some(test_name("Bob Jones"));

        let profile = UserProfile::reconstitute(
            user_id.clone(),
            email.clone(),
            name.clone(),
            first_seen,
            last_seen,
        );

        assert_eq!(profile.user_id, user_id);
        assert_eq!(profile.email, email);
        assert_eq!(profile.name, name);
        assert_eq!(profile.first_seen_at, first_seen);
        assert_eq!(profile.last_seen_at, last_seen);
    }

    #[test]
    fn test_sync_claims_detects_changes_and_updates_last_seen() {
        let t0 = Utc::now() - Duration::hours(2);
        let t1 = Utc::now();
        let mut profile = UserProfile::new(
            test_user_id(),
            Some(test_email("initial@example.com")),
            Some(test_name("Initial Name")),
            t0,
        );

        let new_email = Some(test_email("updated@example.com"));
        let new_name = Some(test_name("Updated Name"));

        let changed = profile.sync_claims(new_email.clone(), new_name.clone(), t1);

        assert!(changed);
        assert_eq!(profile.email, new_email);
        assert_eq!(profile.name, new_name);
        assert_eq!(profile.first_seen_at, t0);
        assert_eq!(profile.last_seen_at, t1);
    }

    #[test]
    fn test_sync_claims_without_changes_returns_false_and_updates_last_seen() {
        let t0 = Utc::now() - Duration::hours(2);
        let t1 = Utc::now();
        let email = Some(test_email("same@example.com"));
        let name = Some(test_name("Same Name"));
        let mut profile = UserProfile::new(test_user_id(), email.clone(), name.clone(), t0);

        let changed = profile.sync_claims(email.clone(), name.clone(), t1);

        assert!(!changed);
        assert_eq!(profile.email, email);
        assert_eq!(profile.name, name);
        assert_eq!(profile.first_seen_at, t0);
        assert_eq!(profile.last_seen_at, t1);
    }

    #[test]
    fn test_touch_advances_last_seen_only() {
        let t0 = Utc::now() - Duration::hours(5);
        let t1 = Utc::now();
        let mut profile = UserProfile::new(test_user_id(), None, None, t0);

        profile.touch(t1);

        assert_eq!(profile.first_seen_at, t0);
        assert_eq!(profile.last_seen_at, t1);
    }
}
