use chrono::{DateTime, Utc};

use super::access_request::AccessRequest;
use super::role::UserRoleAssignment;
use super::user::UserId;
use crate::common::{DisplayName, Email};
use crate::errors::DomainError;

/// Domain policy defining eligibility rules for submitting access requests.
pub struct AccessRequestPolicy;

impl AccessRequestPolicy {
    /// Validates whether a user is eligible to submit an access request.
    ///
    /// # Domain Invariants:
    /// 1. **Enrollment Check**: A user who is already enrolled (possessing one or more
    ///    `UserRoleAssignment` records) cannot submit an access request. Access requests are
    ///    strictly for onboarding non-enrolled users into Luminair.
    /// 2. **Active Request Check**: A user cannot submit an access request if they already
    ///    have an active (Pending or Approved) request in the system.
    ///
    /// # Returns:
    /// - `Ok(())` if the user is eligible to submit a new access request.
    /// - `Err(DomainError::UserAlreadyEnrolled)` if the user already has role assignments.
    /// - `Err(DomainError::AccessRequestAlreadyActive)` if an active request already exists.
    pub fn validate_can_submit(
        user_id: &UserId,
        existing_assignments: &[UserRoleAssignment],
        existing_request: Option<&AccessRequest>,
    ) -> Result<(), DomainError> {
        if !existing_assignments.is_empty() {
            return Err(DomainError::UserAlreadyEnrolled(user_id.clone()));
        }

        if let Some(req) = existing_request
            && req.is_active()
        {
            return Err(DomainError::AccessRequestAlreadyActive(user_id.clone()));
        }

        Ok(())
    }

    /// Guarded factory method for creating a new `AccessRequest`.
    ///
    /// This is the primary and only entry point for instantiating an access request.
    /// It enforces that:
    /// 1. The user must not already be enrolled (`existing_assignments` must be empty).
    /// 2. The user must not already have an active (`Pending` or `Approved`) access request.
    pub fn create_request(
        user_id: UserId,
        email: Option<Email>,
        name: Option<DisplayName>,
        existing_assignments: &[UserRoleAssignment],
        existing_request: Option<&AccessRequest>,
        now: DateTime<Utc>,
    ) -> Result<AccessRequest, DomainError> {
        Self::validate_can_submit(&user_id, existing_assignments, existing_request)?;
        Ok(AccessRequest::new_unvalidated(user_id, email, name, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    use crate::auth::role::{RoleId, UserRoleAssignmentId};
    use crate::auth::AccessRequestStatus;
    use crate::test_support::{fixture_access_request, test_user_id};

    fn make_test_assignment(user_id: &UserId) -> UserRoleAssignment {
        UserRoleAssignment {
            id: UserRoleAssignmentId::new(Uuid::now_v7()),
            user_id: user_id.clone(),
            role_id: RoleId::new(Uuid::now_v7()),
            granted_at: Utc::now(),
            granted_by: None,
        }
    }

    #[test]
    fn test_can_submit_when_not_enrolled_and_no_prior_request() {
        let user = test_user_id("new_user");
        let result = AccessRequestPolicy::validate_can_submit(&user, &[], None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_can_submit_when_previous_request_was_rejected() {
        let user = test_user_id("reapplying_user");
        let (mut req, _, now) = fixture_access_request("reapplying_user");
        let admin = test_user_id("admin");
        req.reject(admin, Some("needs more info".into()), now)
            .unwrap();
        assert!(!req.is_active());

        let result = AccessRequestPolicy::validate_can_submit(&user, &[], Some(&req));
        assert!(result.is_ok());
    }

    #[test]
    fn test_cannot_submit_when_already_enrolled_single_role() {
        let user = test_user_id("enrolled_user");
        let assignment = make_test_assignment(&user);

        let result = AccessRequestPolicy::validate_can_submit(&user, &[assignment], None);
        assert!(matches!(
            result,
            Err(DomainError::UserAlreadyEnrolled(id)) if id == user
        ));
    }

    #[test]
    fn test_cannot_submit_when_already_enrolled_multiple_roles() {
        let user = test_user_id("admin_user");
        let a1 = make_test_assignment(&user);
        let a2 = make_test_assignment(&user);

        let result = AccessRequestPolicy::validate_can_submit(&user, &[a1, a2], None);
        assert!(matches!(
            result,
            Err(DomainError::UserAlreadyEnrolled(id)) if id == user
        ));
    }

    #[test]
    fn test_cannot_submit_when_pending_request_exists() {
        let user = test_user_id("applicant");
        let (req, _, _) = fixture_access_request("applicant");
        assert!(req.is_active());

        let result = AccessRequestPolicy::validate_can_submit(&user, &[], Some(&req));
        assert!(matches!(
            result,
            Err(DomainError::AccessRequestAlreadyActive(id)) if id == user
        ));
    }

    #[test]
    fn test_cannot_submit_when_approved_request_exists() {
        let user = test_user_id("approved_user");
        let (mut req, _, now) = fixture_access_request("approved_user");
        let admin = test_user_id("admin");
        let role = RoleId::new(Uuid::now_v7());
        req.approve(admin, vec![role], now).unwrap();
        assert!(req.is_active());

        let result = AccessRequestPolicy::validate_can_submit(&user, &[], Some(&req));
        assert!(matches!(
            result,
            Err(DomainError::AccessRequestAlreadyActive(id)) if id == user
        ));
    }

    #[test]
    fn test_enrolled_check_takes_precedence_over_active_request() {
        let user = test_user_id("enrolled_and_requested");
        let assignment = make_test_assignment(&user);
        let (req, _, _) = fixture_access_request("enrolled_and_requested");

        let result = AccessRequestPolicy::validate_can_submit(&user, &[assignment], Some(&req));
        assert!(matches!(
            result,
            Err(DomainError::UserAlreadyEnrolled(id)) if id == user
        ));
    }

    #[test]
    fn test_create_request_success() {
        let user = test_user_id("valid_applicant");
        let email = Email::try_new("applicant@example.com").ok();
        let name = DisplayName::try_new("Applicant User").ok();
        let now = Utc::now();

        let req = AccessRequestPolicy::create_request(
            user.clone(),
            email.clone(),
            name.clone(),
            &[],
            None,
            now,
        )
        .expect("should successfully create request");

        assert_eq!(req.user_id, user);
        assert_eq!(req.email, email);
        assert_eq!(req.name, name);
        assert_eq!(req.requested_at, now);
        assert!(req.is_active());
        assert!(matches!(req.status, AccessRequestStatus::Pending));
        assert!(req.assigned_roles.is_empty());
    }

    #[test]
    fn test_create_request_enforces_enrollment_invariants() {
        let user = test_user_id("enrolled_user");
        let assignment = make_test_assignment(&user);
        let now = Utc::now();

        let err = AccessRequestPolicy::create_request(
            user.clone(),
            None,
            None,
            &[assignment],
            None,
            now,
        )
        .unwrap_err();

        assert!(matches!(err, DomainError::UserAlreadyEnrolled(id) if id == user));
    }
}
