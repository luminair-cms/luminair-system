//! Spec: docs/specs/application/access-requests-service.md — declarative behavior specification rules.

use chrono::Utc;
use uuid::Uuid;

use application::commands::access_requests::*;
use application::context::CallerContext;
use application::errors::ApplicationError;
use application::services::{AccessRequestsService, AccessRequestsServiceImpl};
use application::test_support::{
    FakeAccessRequestRepository, FakeRoleRepository, FakeUserRoleAssignmentRepository,
};
use domain::auth::{
    AccessRequestRepository, AccessRequestStatus, ActorId, Role, RoleId, RoleRepository,
    UserRoleAssignment, UserRoleAssignmentId, UserRoleAssignmentRepository,
};
use domain::common::{DisplayName, Email};
use domain::errors::DomainError;
use domain::test_support::{admin_role, create_user_id};

type TestAccessRequestsService = AccessRequestsServiceImpl<
    FakeAccessRequestRepository,
    FakeUserRoleAssignmentRepository,
    FakeRoleRepository,
>;

fn create_test_service() -> TestAccessRequestsService {
    let access_repo = FakeAccessRequestRepository::new();
    let assignment_repo = FakeUserRoleAssignmentRepository::new();
    let role_repo = FakeRoleRepository::new();
    AccessRequestsServiceImpl::new(access_repo, assignment_repo, role_repo)
}

#[tokio::test]
async fn ar_01_submit_creates_pending_request_for_new_user() {
    let service = create_test_service();
    let user = create_user_id("user-01");

    let req = service
        .submit(SubmitAccessRequestCommand::new(
            user.clone(),
            Email::try_new("new@example.com").ok(),
            DisplayName::try_new("New User").ok(),
        ))
        .await
        .expect("AR-01: submit should succeed for new user");

    assert_eq!(req.user_id, user);
    assert!(matches!(req.status, AccessRequestStatus::Pending));

    let stored = service
        .access_request_repo
        .find_by_id(req.id)
        .await
        .unwrap()
        .expect("AR-01: request should be saved in repository");
    assert_eq!(stored.id, req.id);
    assert_eq!(stored.user_id, user);
}

#[tokio::test]
async fn ar_02_submit_rejected_if_user_already_enrolled() {
    let service = create_test_service();
    let user = create_user_id("user-02");

    let assignment = UserRoleAssignment {
        id: UserRoleAssignmentId::new(Uuid::now_v7()),
        user_id: user.clone(),
        role_id: RoleId::new(Uuid::now_v7()),
        granted_at: Utc::now(),
        granted_by: None,
    };
    service.assignment_repo.save(&assignment).await.unwrap();

    let result = service
        .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
        .await;

    assert!(matches!(
        result,
        Err(ApplicationError::Domain(DomainError::UserAlreadyEnrolled(u))) if u == user
    ));
}

#[tokio::test]
async fn ar_03_submit_rejected_if_active_request_exists() {
    let service = create_test_service();
    let user = create_user_id("user-03");

    service
        .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
        .await
        .expect("AR-03: first submit should succeed");

    let second = service
        .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
        .await;

    assert!(matches!(
        second,
        Err(ApplicationError::Domain(DomainError::AccessRequestAlreadyActive(u))) if u == user
    ));
}

#[tokio::test]
async fn ar_04_submit_allowed_if_previous_request_was_rejected() {
    let service = create_test_service();
    let user = create_user_id("user-04");
    let admin_caller = CallerContext::system();

    let first = service
        .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
        .await
        .expect("AR-04: first submit should succeed");

    service
        .reject(
            &admin_caller,
            RejectAccessRequestCommand::new(first.id, Some("Declined".into())),
        )
        .await
        .expect("AR-04: rejection should succeed");

    let second = service
        .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
        .await
        .expect("AR-04: second submit should succeed after rejection");

    assert!(matches!(second.status, AccessRequestStatus::Pending));
    assert_eq!(second.user_id, user);
    assert_ne!(first.id, second.id);
}

#[tokio::test]
async fn ar_05_approve_creates_role_assignments_and_sets_status() {
    let service = create_test_service();
    let user = create_user_id("candidate-05");
    let admin_user = create_user_id("admin-05");
    let admin_caller = CallerContext::new(ActorId::User(admin_user.clone()), vec![admin_role()]);

    let r1 = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "role-1".into(),
        description: None,
        permissions: vec![],
    };
    let r2 = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "role-2".into(),
        description: None,
        permissions: vec![],
    };
    service.role_repo.save(&r1).await.unwrap();
    service.role_repo.save(&r2).await.unwrap();

    let req = service
        .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
        .await
        .expect("AR-05: submit should succeed");

    let assignments = service
        .approve(
            &admin_caller,
            ApproveAccessRequestCommand::new(req.id, vec![r1.id, r2.id]),
        )
        .await
        .expect("AR-05: approve should succeed");

    assert_eq!(assignments.len(), 2);
    assert_eq!(
        assignments[0].granted_by,
        Some(ActorId::User(admin_user.clone()))
    );
    assert_eq!(
        assignments[1].granted_by,
        Some(ActorId::User(admin_user.clone()))
    );

    let updated_req = service
        .access_request_repo
        .find_by_id(req.id)
        .await
        .unwrap()
        .expect("AR-05: request should exist");

    assert!(matches!(updated_req.status, AccessRequestStatus::Approved));
    assert_eq!(updated_req.reviewed_by, Some(ActorId::User(admin_user)));
}

#[tokio::test]
async fn ar_06_approve_rejects_self_approval() {
    let service = create_test_service();
    let admin_user = create_user_id("admin-06");
    let admin_caller = CallerContext::new(ActorId::User(admin_user.clone()), vec![admin_role()]);

    let role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "editor".into(),
        description: None,
        permissions: vec![],
    };
    service.role_repo.save(&role).await.unwrap();

    let req = service
        .submit(SubmitAccessRequestCommand::new(
            admin_user.clone(),
            None,
            None,
        ))
        .await
        .expect("AR-06: submit should succeed");

    let result = service
        .approve(
            &admin_caller,
            ApproveAccessRequestCommand::new(req.id, vec![role.id]),
        )
        .await;

    assert!(matches!(
        result,
        Err(ApplicationError::Domain(DomainError::Unauthorized(_)))
    ));
}

#[tokio::test]
async fn ar_07_approve_rejects_non_existent_role_id() {
    let service = create_test_service();
    let user = create_user_id("user-07");
    let admin_user = create_user_id("admin-07");
    let admin_caller = CallerContext::new(ActorId::User(admin_user), vec![admin_role()]);

    let req = service
        .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
        .await
        .expect("AR-07: submit should succeed");

    let non_existent_role = RoleId::new(Uuid::now_v7());
    let result = service
        .approve(
            &admin_caller,
            ApproveAccessRequestCommand::new(req.id, vec![non_existent_role]),
        )
        .await;

    assert!(matches!(
        result,
        Err(ApplicationError::NotFound { entity: "Role", .. })
    ));

    let unmutated_req = service
        .access_request_repo
        .find_by_id(req.id)
        .await
        .unwrap()
        .expect("AR-07: request should exist");
    assert!(matches!(unmutated_req.status, AccessRequestStatus::Pending));
}

#[tokio::test]
async fn ar_08_reject_records_reason_and_reviewer() {
    let service = create_test_service();
    let user = create_user_id("user-08");
    let admin_user = create_user_id("admin-08");
    let admin_caller = CallerContext::new(ActorId::User(admin_user.clone()), vec![admin_role()]);

    let req = service
        .submit(SubmitAccessRequestCommand::new(user.clone(), None, None))
        .await
        .expect("AR-08: submit should succeed");

    let rejected = service
        .reject(
            &admin_caller,
            RejectAccessRequestCommand::new(req.id, Some("Incomplete info".into())),
        )
        .await
        .expect("AR-08: reject should succeed");

    assert!(matches!(
        rejected.status,
        AccessRequestStatus::Rejected { ref reason } if reason.as_deref() == Some("Incomplete info")
    ));
    assert_eq!(
        rejected.reviewed_by,
        Some(ActorId::User(admin_user.clone()))
    );

    let stored = service
        .access_request_repo
        .find_by_id(req.id)
        .await
        .unwrap()
        .expect("AR-08: request should exist");
    assert!(matches!(
        stored.status,
        AccessRequestStatus::Rejected { ref reason } if reason.as_deref() == Some("Incomplete info")
    ));
    assert_eq!(stored.reviewed_by, Some(ActorId::User(admin_user)));
}

#[tokio::test]
async fn ar_09_requester_can_read_own_request_without_admin_role() {
    let service = create_test_service();
    let user_a = create_user_id("user-a-09");
    let caller_a = CallerContext::new(ActorId::User(user_a.clone()), vec![]);

    let req = service
        .submit(SubmitAccessRequestCommand::new(user_a.clone(), None, None))
        .await
        .expect("AR-09: submit should succeed");

    let fetched = service
        .get_by_id(&caller_a, req.id)
        .await
        .expect("AR-09: requester should be able to read own request");

    assert_eq!(fetched.id, req.id);
    assert_eq!(fetched.user_id, user_a);
}

#[tokio::test]
async fn ar_10_non_requester_denied_get_without_manage_users_permission() {
    let service = create_test_service();
    let user_a = create_user_id("user-a-10");
    let user_b = create_user_id("user-b-10");
    let caller_b = CallerContext::new(ActorId::User(user_b), vec![]);

    let req = service
        .submit(SubmitAccessRequestCommand::new(user_a.clone(), None, None))
        .await
        .expect("AR-10: submit should succeed");

    let result = service.get_by_id(&caller_b, req.id).await;

    assert!(matches!(result, Err(ApplicationError::Unauthorized { .. })));
}
