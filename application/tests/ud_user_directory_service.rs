//! Integration tests for UserDirectoryService.

use application::commands::user_directory::*;
use application::context::CallerContext;
use application::errors::ApplicationError;
use application::services::{UserDirectoryService, UserDirectoryServiceImpl};
use application::test_support::FakeUserProfileRepository;
use domain::auth::role::Role;
use domain::auth::{Permission, RoleId, UserProfile};
use domain::common::{DisplayName, Email, Pagination};
use domain::test_support::{admin_role, create_user_id, fixture_user_profile};
use uuid::Uuid;

type TestUserDirectoryService = UserDirectoryServiceImpl<FakeUserProfileRepository>;

fn create_test_service() -> (TestUserDirectoryService, FakeUserProfileRepository) {
    let repo = FakeUserProfileRepository::new();
    let service = UserDirectoryServiceImpl::new(repo.clone());
    (service, repo)
}

fn user_caller(name: &str, roles: Vec<Role>) -> CallerContext {
    CallerContext::new(create_user_id(name), roles)
}

fn manager_role() -> Role {
    Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "user_manager".into(),
        description: None,
        permissions: vec![Permission::ManageUsers],
    }
}

#[tokio::test]
async fn ud_01_list_users_requires_manage_users_permission() {
    let (service, _) = create_test_service();
    let unprivileged_caller = user_caller("alice", vec![]);
    let manager_caller = user_caller("bob", vec![manager_role()]);
    let admin_caller = user_caller("superadmin", vec![admin_role()]);

    let cmd = ListUsersCommand::default();

    // 1. Unprivileged user is denied
    let err = service
        .list_users(&unprivileged_caller, cmd.clone())
        .await
        .unwrap_err();
    assert!(matches!(err, ApplicationError::Unauthorized { .. }));

    // 2. Manager with ManageUsers is permitted
    let res_manager = service.list_users(&manager_caller, cmd.clone()).await;
    assert!(res_manager.is_ok());

    // 3. Admin is permitted
    let res_admin = service.list_users(&admin_caller, cmd).await;
    assert!(res_admin.is_ok());
}

#[tokio::test]
async fn ud_02_list_users_filters_by_name_and_email_and_supports_pagination() {
    let (service, repo) = create_test_service();
    let admin_caller = user_caller("admin", vec![admin_role()]);

    // Seed 4 user profiles
    let u1 = fixture_user_profile("alice"); // alice@example.com, Alice
    let u2 = fixture_user_profile("bob"); // bob@example.com, Bob
    let u3 = UserProfile::new(
        create_user_id("charlie"),
        Email::try_new("charlie@acme.org").ok(),
        DisplayName::try_new("Charlie Smith").ok(),
        chrono::Utc::now(),
    );
    let u4 = UserProfile::new(
        create_user_id("david"),
        Email::try_new("david@acme.org").ok(),
        DisplayName::try_new("David Smith").ok(),
        chrono::Utc::now(),
    );

    domain::auth::UserProfileRepository::save(&repo, &u1).await.unwrap();
    domain::auth::UserProfileRepository::save(&repo, &u2).await.unwrap();
    domain::auth::UserProfileRepository::save(&repo, &u3).await.unwrap();
    domain::auth::UserProfileRepository::save(&repo, &u4).await.unwrap();

    // 1. Filter by domain "acme.org"
    let page_domain = service
        .list_users(
            &admin_caller,
            ListUsersCommand::new(Some("acme.org".into()), Pagination::new(1, 10)),
        )
        .await
        .expect("listing succeeds");
    assert_eq!(page_domain.total, 2);
    assert_eq!(page_domain.items.len(), 2);
    assert_eq!(page_domain.items[0].user_id.as_str(), "charlie");
    assert_eq!(page_domain.items[1].user_id.as_str(), "david");

    // 2. Filter by name "Smith" with page size 1
    let page_smith_p1 = service
        .list_users(
            &admin_caller,
            ListUsersCommand::new(Some("smith".into()), Pagination::new(1, 1)),
        )
        .await
        .expect("page 1 succeeds");
    assert_eq!(page_smith_p1.total, 2);
    assert_eq!(page_smith_p1.items.len(), 1);
    assert_eq!(page_smith_p1.items[0].user_id.as_str(), "charlie");

    let page_smith_p2 = service
        .list_users(
            &admin_caller,
            ListUsersCommand::new(Some("smith".into()), Pagination::new(2, 1)),
        )
        .await
        .expect("page 2 succeeds");
    assert_eq!(page_smith_p2.total, 2);
    assert_eq!(page_smith_p2.items.len(), 1);
    assert_eq!(page_smith_p2.items[0].user_id.as_str(), "david");
}

#[tokio::test]
async fn ud_03_get_user_allows_self_lookup_without_manage_users_permission() {
    let (service, repo) = create_test_service();
    let alice_id = create_user_id("alice");
    let u1 = fixture_user_profile("alice");
    domain::auth::UserProfileRepository::save(&repo, &u1).await.unwrap();

    let alice_caller = user_caller("alice", vec![]);
    let profile = service
        .get_user(&alice_caller, &alice_id)
        .await
        .expect("self lookup succeeds")
        .expect("profile exists");

    assert_eq!(profile.user_id, alice_id);
}

#[tokio::test]
async fn ud_04_get_user_denies_other_user_without_manage_users_permission() {
    let (service, repo) = create_test_service();
    let bob_id = create_user_id("bob");
    let u_bob = fixture_user_profile("bob");
    domain::auth::UserProfileRepository::save(&repo, &u_bob).await.unwrap();

    let alice_caller = user_caller("alice", vec![]);
    let err = service
        .get_user(&alice_caller, &bob_id)
        .await
        .unwrap_err();

    assert!(matches!(err, ApplicationError::Unauthorized { .. }));
}

#[tokio::test]
async fn ud_05_get_user_allows_admin_to_lookup_any_user() {
    let (service, repo) = create_test_service();
    let bob_id = create_user_id("bob");
    let u_bob = fixture_user_profile("bob");
    domain::auth::UserProfileRepository::save(&repo, &u_bob).await.unwrap();

    let admin_caller = user_caller("admin", vec![admin_role()]);
    let profile = service
        .get_user(&admin_caller, &bob_id)
        .await
        .expect("admin lookup succeeds")
        .expect("profile exists");

    assert_eq!(profile.user_id, bob_id);
}

#[tokio::test]
async fn ud_06_sync_profile_creates_new_user_profile() {
    let (service, _) = create_test_service();
    let user_id = create_user_id("new_user");
    let email = Email::try_new("new_user@example.com").ok();
    let name = DisplayName::try_new("New User").ok();

    let profile = service
        .sync_profile(SyncProfileCommand::new(user_id.clone(), email.clone(), name.clone()))
        .await
        .expect("sync profile creates profile");

    assert_eq!(profile.user_id, user_id);
    assert_eq!(profile.email, email);
    assert_eq!(profile.name, name);
    assert_eq!(profile.first_seen_at, profile.last_seen_at);
}

#[tokio::test]
async fn ud_07_sync_profile_updates_existing_profile_and_preserves_first_seen_at() {
    let (service, _) = create_test_service();
    let user_id = create_user_id("existing_user");
    let email1 = Email::try_new("initial@example.com").ok();
    let name1 = DisplayName::try_new("Initial Name").ok();

    let p1 = service
        .sync_profile(SyncProfileCommand::new(user_id.clone(), email1, name1))
        .await
        .expect("first sync creates profile");

    let email2 = Email::try_new("updated@example.com").ok();
    let name2 = DisplayName::try_new("Updated Name").ok();

    let p2 = service
        .sync_profile(SyncProfileCommand::new(user_id.clone(), email2.clone(), name2.clone()))
        .await
        .expect("second sync updates claims");

    assert_eq!(p2.user_id, user_id);
    assert_eq!(p2.email, email2);
    assert_eq!(p2.name, name2);
    assert_eq!(p2.first_seen_at, p1.first_seen_at);
    assert!(p2.last_seen_at >= p1.last_seen_at);
}
