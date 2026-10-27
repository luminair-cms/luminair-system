//! End-to-end HTTP integration test suite for the REST API driving adapter.
//!
//! Validates health probes, OIDC & service token authentication, onboarding states,
//! RFC 9457 Problem Details errors, schema inspection, and content publication lifecycle.

use std::sync::{Arc, OnceLock};

use axum::http::StatusCode;
use axum_test::TestServer;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use domain::auth::ports::{RoleRepository, UserRoleAssignmentRepository};
use domain::auth::role::{Permission, Role, RoleId, UserRoleAssignment, UserRoleAssignmentId};
use domain::auth::user::{ServiceId, UserId};
use domain::schema::attributes::{AttributeId, RelationDefinition, RelationType};
use domain::schema::documents::DocumentTypeId;
use domain::schema::registry::SchemaRegistry;
use domain::system::config::{SystemConfig, SystemConfigId};
use domain::system::context::SystemContext;
use domain::system::locale::LocaleId;
use domain::test_support::builders::{DocumentTypeBuilder, text_field};
use domain::test_support::fixtures::admin_role;

use application::services::{
    AccessRequestsServiceImpl, DocumentsServiceImpl, SchemaServiceImpl, SystemConfigServiceImpl,
};
use application::test_support::{
    FakeAccessRequestRepository, FakeDocumentInstanceRepository, FakeRoleRepository,
    FakeUserRoleAssignmentRepository,
};

use infrastructure::api::routes::build_router;
use infrastructure::api::state::HttpState;
use infrastructure::auth::{DefaultAuthResolver, OidcTokenValidator, ServiceTokenRegistry};
use infrastructure::health::AlwaysHealthyChecker;

// ---------------------------------------------------------------------------
// Constants First (Newspaper Style)
// ---------------------------------------------------------------------------

const TEST_JWT_SECRET: &[u8] = b"luminair-super-secret-jwt-key-32bytes-minimum";
const PROBLEM_CONTENT_TYPE: &str = "application/problem+json";

static SYSTEM_CONTEXT: OnceLock<&'static SystemContext> = OnceLock::new();

// ---------------------------------------------------------------------------
// Test Fixture Types
// ---------------------------------------------------------------------------

/// In-process HTTP test fixture holding a running `TestServer` and test credentials.
struct TestFixture {
    server: TestServer,
    admin_token: String,
    service_token: String,
    editor_role_id: RoleId,
}

// ---------------------------------------------------------------------------
// Inherent Implementations
// ---------------------------------------------------------------------------

impl TestFixture {
    /// Bootstraps an isolated in-memory HTTP API server with seeded admin and roles.
    async fn new() -> Self {
        let context = get_system_context();

        let doc_repo = FakeDocumentInstanceRepository::new();
        let access_repo = FakeAccessRequestRepository::new();
        let assignment_repo = FakeUserRoleAssignmentRepository::new();
        let role_repo = FakeRoleRepository::new();

        let admin_r = admin_role();
        let admin_role_id = admin_r.id;

        let editor_role_id = RoleId::new(Uuid::now_v7());
        let editor_r = Role {
            id: editor_role_id,
            name: "editor".into(),
            description: Some("Content Editor".into()),
            permissions: vec![
                Permission::CreateDocument(None),
                Permission::ReadDocument(None),
                Permission::UpdateDocument(None),
                Permission::DeleteDocument(None),
                Permission::PublishDocument(None),
            ],
        };

        role_repo.save(&admin_r).await.expect("save admin role");
        role_repo.save(&editor_r).await.expect("save editor role");

        let admin_user_id = UserId::try_new("usr_admin").expect("valid admin user id");
        let admin_assignment = UserRoleAssignment {
            id: UserRoleAssignmentId::new(Uuid::now_v7()),
            user_id: admin_user_id.clone(),
            role_id: admin_role_id,
            granted_at: Utc::now(),
            granted_by: None,
        };
        assignment_repo
            .save_all(&[admin_assignment])
            .await
            .expect("save admin assignment");

        let service_token = "secret-service-m2m-token-987".to_string();
        let mut service_tokens = ServiceTokenRegistry::new();
        let service_id = ServiceId::try_new("admin-daemon").expect("valid service id");
        service_tokens.register(&service_token, service_id, vec![admin_role_id]);

        let oidc_validator = Arc::new(OidcTokenValidator::new_hs256(
            TEST_JWT_SECRET,
            None,
            None,
        ));

        let resolver = Arc::new(DefaultAuthResolver::without_shadow_recorder(
            oidc_validator,
            Arc::new(service_tokens),
            access_repo.clone(),
            assignment_repo.clone(),
            role_repo.clone(),
        ));

        let documents_service = Arc::new(DocumentsServiceImpl::new(doc_repo, context));
        let access_requests_service = Arc::new(AccessRequestsServiceImpl::new(
            access_repo,
            assignment_repo,
            role_repo,
        ));
        let schema_service = Arc::new(SchemaServiceImpl::new(context));
        let system_config_service = Arc::new(SystemConfigServiceImpl::new(context));
        let health_checker = Arc::new(AlwaysHealthyChecker);

        let state = HttpState::new(
            context,
            resolver,
            documents_service,
            access_requests_service,
            schema_service,
            system_config_service,
            health_checker,
        );

        let router = build_router(state);
        let server = TestServer::new(router);

        let admin_token = create_test_jwt(
            admin_user_id.as_ref(),
            Some("admin@luminair.io"),
            Some("System Administrator"),
        );

        Self {
            server,
            admin_token,
            service_token,
            editor_role_id,
        }
    }

    /// Formats the admin Bearer authentication header.
    fn admin_bearer(&self) -> String {
        format!("Bearer {}", self.admin_token)
    }

    /// Formats the machine-to-machine service Bearer authentication header.
    fn service_bearer(&self) -> String {
        format!("Bearer {}", self.service_token)
    }

    /// Formats a human user Bearer authentication header.
    fn user_bearer(&self, sub: &str, email: &str, name: &str) -> String {
        let token = create_test_jwt(sub, Some(email), Some(name));
        format!("Bearer {token}")
    }
}

// ---------------------------------------------------------------------------
// Helper Functions (Newspaper Ordering)
// ---------------------------------------------------------------------------

fn get_system_context() -> &'static SystemContext {
    SYSTEM_CONTEXT.get_or_init(|| Box::leak(Box::new(build_system_context())))
}

fn build_system_context() -> SystemContext {
    let en = LocaleId::try_new("en").expect("valid locale en");
    let uk = LocaleId::try_new("uk").expect("valid locale uk");
    let config = SystemConfig::new(
        SystemConfigId::new(Uuid::now_v7()),
        vec![en.clone(), uk],
        en,
    )
    .expect("valid system config");

    let tags_type = DocumentTypeBuilder::collection("tags")
        .with_title("Tags")
        .with_field(text_field("name").required(true))
        .with_display_fields(vec![AttributeId::try_new("name").expect("valid name attr")])
        .build();

    let articles_type = DocumentTypeBuilder::collection("articles")
        .with_title("Articles")
        .with_field(text_field("title").required(true))
        .with_field(text_field("content").required(false))
        .with_display_fields(vec![AttributeId::try_new("title").expect("valid title attr")])
        .with_relation(RelationDefinition {
            id: AttributeId::try_new("tags").expect("valid tags attr"),
            relation_type: RelationType::HasMany,
            target: DocumentTypeId::try_new("tags").expect("valid tags type id"),
        })
        .build();

    let registry = SchemaRegistry::new(vec![articles_type, tags_type]);
    SystemContext::new(registry, config)
}

fn create_test_jwt(sub: &str, email: Option<&str>, name: Option<&str>) -> String {
    let exp = (Utc::now() + chrono::Duration::hours(24)).timestamp() as usize;
    OidcTokenValidator::encode_hs256_token(TEST_JWT_SECRET, sub, email, name, exp)
        .expect("token encoding must succeed")
}

fn create_expired_jwt(sub: &str) -> String {
    let exp = (Utc::now() - chrono::Duration::hours(2)).timestamp() as usize;
    OidcTokenValidator::encode_hs256_token(TEST_JWT_SECRET, sub, None, None, exp)
        .expect("expired token encoding must succeed")
}

// ---------------------------------------------------------------------------
// Integration Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_01_health_and_readiness_probes_and_request_id_propagation() {
    let fixture = TestFixture::new().await;

    // 1. GET /healthz returns 200 OK and generates an X-Request-Id header
    let res_healthz = fixture.server.get("/healthz").await;
    res_healthz.assert_status_ok();
    res_healthz.assert_json(&json!({"status": "healthy"}));
    assert!(res_healthz.maybe_header("x-request-id").is_some());

    // 2. GET /ready returns 200 OK
    let res_ready = fixture.server.get("/ready").await;
    res_ready.assert_status_ok();
    res_ready.assert_json(&json!({"status": "healthy"}));

    // 3. Client-provided x-request-id is preserved on response
    let custom_id = "test-custom-request-id-42";
    let res_custom = fixture
        .server
        .get("/healthz")
        .add_header("x-request-id", custom_id)
        .await;
    res_custom.assert_status_ok();
    res_custom.assert_header("x-request-id", custom_id);
}

#[tokio::test]
async fn test_02_authentication_and_unauthenticated_rfc9457_errors() {
    let fixture = TestFixture::new().await;

    // 1. Missing Authorization header -> 401 with RFC 9457 problem details
    let res_missing = fixture.server.get("/api/v1/system/config").await;
    res_missing.assert_status_unauthorized();
    assert!(
        res_missing
            .content_type()
            .starts_with(PROBLEM_CONTENT_TYPE)
    );

    let body_missing: Value = res_missing.json();
    assert_eq!(body_missing["code"], "unauthenticated");
    assert_eq!(body_missing["status"], 401);
    assert_eq!(body_missing["instance"], "/api/v1/system/config");

    // 2. Invalid JWT token signature / malformed token -> 401
    let res_invalid = fixture
        .server
        .get("/api/v1/system/config")
        .add_header("authorization", "Bearer not-a-valid-jwt-token")
        .await;
    res_invalid.assert_status_unauthorized();

    let body_invalid: Value = res_invalid.json();
    assert_eq!(body_invalid["code"], "unauthenticated");

    // 3. Expired JWT token -> 401
    let expired_token = create_expired_jwt("usr_alice");
    let res_expired = fixture
        .server
        .get("/api/v1/system/config")
        .add_header("authorization", format!("Bearer {expired_token}"))
        .await;
    res_expired.assert_status_unauthorized();

    let body_expired: Value = res_expired.json();
    assert_eq!(body_expired["code"], "unauthenticated");
}

#[tokio::test]
async fn test_03_onboarding_lifecycle_and_rbac_flow() {
    let fixture = TestFixture::new().await;
    let alice_bearer = fixture.user_bearer("usr_alice", "alice@example.com", "Alice Wonder");

    // 1. Alice with no roles & no request calling protected endpoint -> 403 access_not_requested
    let res_not_req = fixture
        .server
        .get("/api/v1/system/config")
        .add_header("authorization", &alice_bearer)
        .await;
    res_not_req.assert_status_forbidden();
    let body_not_req: Value = res_not_req.json();
    assert_eq!(body_not_req["code"], "access_not_requested");
    assert_eq!(body_not_req["details"]["user_id"], "usr_alice");

    // 2. Alice submits an access request -> 201 Created with status pending
    let res_submit = fixture
        .server
        .post("/api/v1/auth/access-requests")
        .add_header("authorization", &alice_bearer)
        .await;
    res_submit.assert_status(StatusCode::CREATED);
    let submit_body: Value = res_submit.json();
    let alice_req_id = submit_body["data"]["id"]
        .as_str()
        .expect("access request id")
        .to_string();
    assert_eq!(submit_body["data"]["userId"], "usr_alice");
    assert_eq!(submit_body["data"]["status"]["status"], "pending");

    // 3. Alice with pending request calling protected endpoint -> 403 access_pending
    let res_pending = fixture
        .server
        .get("/api/v1/system/config")
        .add_header("authorization", &alice_bearer)
        .await;
    res_pending.assert_status_forbidden();
    let body_pending: Value = res_pending.json();
    assert_eq!(body_pending["code"], "access_pending");
    assert_eq!(body_pending["details"]["user_id"], "usr_alice");
    assert!(body_pending["details"]["submitted_at"].is_string());

    // 4. Service token caller lists pending requests and inspects Alice's request
    let res_list = fixture
        .server
        .get("/api/v1/auth/access-requests")
        .add_header("authorization", fixture.service_bearer())
        .await;
    res_list.assert_status_ok();
    let list_body: Value = res_list.json();
    assert!(
        list_body["data"]
            .as_array()
            .expect("data array")
            .iter()
            .any(|r| r["id"] == alice_req_id)
    );

    // 5. Service token caller fetches Alice's request by ID
    let res_get_req = fixture
        .server
        .get(&format!("/api/v1/auth/access-requests/{alice_req_id}"))
        .add_header("authorization", fixture.service_bearer())
        .await;
    res_get_req.assert_status_ok();
    let get_req_body: Value = res_get_req.json();
    assert_eq!(get_req_body["data"]["id"], alice_req_id);

    // 6. Service token rejects Alice's access request
    let res_reject = fixture
        .server
        .post(&format!(
            "/api/v1/auth/access-requests/{alice_req_id}/reject"
        ))
        .add_header("authorization", fixture.service_bearer())
        .json(&json!({"reason": "Wrong department"}))
        .await;
    res_reject.assert_status_ok();
    let reject_body: Value = res_reject.json();
    assert_eq!(reject_body["data"]["status"]["status"], "rejected");
    assert_eq!(
        reject_body["data"]["status"]["reason"],
        "Wrong department"
    );

    // 7. Rejected Alice calling protected endpoint -> 403 access_rejected
    let res_rejected = fixture
        .server
        .get("/api/v1/system/config")
        .add_header("authorization", &alice_bearer)
        .await;
    res_rejected.assert_status_forbidden();
    let body_rej: Value = res_rejected.json();
    assert_eq!(body_rej["code"], "access_rejected");
    assert_eq!(body_rej["details"]["user_id"], "usr_alice");
    assert_eq!(body_rej["details"]["reason"], "Wrong department");

    // 8. Bob submits access request and Admin approves it
    let bob_bearer = fixture.user_bearer("usr_bob", "bob@example.com", "Bob Builder");
    let res_bob_sub = fixture
        .server
        .post("/api/v1/auth/access-requests")
        .add_header("authorization", &bob_bearer)
        .await;
    res_bob_sub.assert_status(StatusCode::CREATED);
    let bob_req_id = res_bob_sub.json::<Value>()["data"]["id"]
        .as_str()
        .expect("bob request id")
        .to_string();

    let res_approve = fixture
        .server
        .post(&format!("/api/v1/auth/access-requests/{bob_req_id}/approve"))
        .add_header("authorization", fixture.admin_bearer())
        .json(&json!({"roleIds": [*fixture.editor_role_id]}))
        .await;
    res_approve.assert_status_ok();

    // 9. Newly approved Bob can now access protected studio routes
    let res_bob_allowed = fixture
        .server
        .get("/api/v1/system/config")
        .add_header("authorization", &bob_bearer)
        .await;
    res_bob_allowed.assert_status_ok();
    let bob_config: Value = res_bob_allowed.json();
    assert_eq!(bob_config["data"]["defaultLocale"], "en");
}

#[tokio::test]
async fn test_04_system_config_and_locales() {
    let fixture = TestFixture::new().await;
    let auth = fixture.admin_bearer();

    // 1. GET /api/v1/system/config returns default & available locales
    let res_cfg = fixture
        .server
        .get("/api/v1/system/config")
        .add_header("authorization", &auth)
        .await;
    res_cfg.assert_status_ok();
    let cfg_body: Value = res_cfg.json();
    assert_eq!(cfg_body["data"]["defaultLocale"], "en");
    let locales: Vec<String> = cfg_body["data"]["availableLocales"]
        .as_array()
        .expect("available locales")
        .iter()
        .filter_map(|v| v.as_str().map(ToString::to_string))
        .collect();
    assert_eq!(locales, vec!["en", "uk"]);

    // 2. GET /api/v1/system/locales
    let res_loc = fixture
        .server
        .get("/api/v1/system/locales")
        .add_header("authorization", &auth)
        .await;
    res_loc.assert_status_ok();
    let loc_body: Value = res_loc.json();
    assert_eq!(loc_body["data"]["defaultLocale"], "en");
}

#[tokio::test]
async fn test_05_schema_document_types_and_not_found() {
    let fixture = TestFixture::new().await;
    let auth = fixture.admin_bearer();

    // 1. GET /api/v1/schema/document-types returns registered document types
    let res_types = fixture
        .server
        .get("/api/v1/schema/document-types")
        .add_header("authorization", &auth)
        .await;
    res_types.assert_status_ok();
    let types_body: Value = res_types.json();
    assert_eq!(types_body["data"].as_array().expect("array").len(), 2);

    // 2. GET /api/v1/schemas (alias)
    let res_alias = fixture
        .server
        .get("/api/v1/schemas")
        .add_header("authorization", &auth)
        .await;
    res_alias.assert_status_ok();

    // 3. GET /api/v1/schema/document-types/articles returns articles schema DTO
    let res_articles = fixture
        .server
        .get("/api/v1/schema/document-types/articles")
        .add_header("authorization", &auth)
        .await;
    res_articles.assert_status_ok();
    let articles_body: Value = res_articles.json();
    assert_eq!(articles_body["data"]["id"], "articles");

    // 4. GET /api/v1/schemas/articles (alias)
    let res_articles_alias = fixture
        .server
        .get("/api/v1/schemas/articles")
        .add_header("authorization", &auth)
        .await;
    res_articles_alias.assert_status_ok();

    // 5. GET /api/v1/schema/document-types/nonexistent -> 404 document_type_not_found
    let res_404 = fixture
        .server
        .get("/api/v1/schema/document-types/nonexistent")
        .add_header("authorization", &auth)
        .await;
    res_404.assert_status_not_found();
    let err_body: Value = res_404.json();
    assert_eq!(err_body["code"], "document_type_not_found");
    assert_eq!(
        err_body["instance"],
        "/api/v1/schema/document-types/nonexistent"
    );
}

#[tokio::test]
async fn test_06_content_validation_and_occ_conflicts() {
    let fixture = TestFixture::new().await;
    let auth = fixture.admin_bearer();

    // 1. Validation failure: unknown attribute -> 422 with invalid_params
    let res_unknown = fixture
        .server
        .post("/api/v1/content/articles")
        .add_header("authorization", &auth)
        .json(&json!({
            "fields": {
                "title": "Valid Title",
                "unknown-attr": "value"
            }
        }))
        .await;
    res_unknown.assert_status_unprocessable_entity();
    let unknown_body: Value = res_unknown.json();
    assert_eq!(unknown_body["code"], "validation_failed");
    let invalid_params = unknown_body["invalid_params"]
        .as_array()
        .expect("invalid_params");
    assert!(invalid_params.iter().any(|p| p["code"] == "unknown_attribute"));

    // 2. Validation failure: invalid field value type -> 422
    let res_type_err = fixture
        .server
        .post("/api/v1/content/articles")
        .add_header("authorization", &auth)
        .json(&json!({
            "fields": {
                "title": 99999
            }
        }))
        .await;
    res_type_err.assert_status_unprocessable_entity();
    let type_err_body: Value = res_type_err.json();
    assert_eq!(type_err_body["code"], "validation_failed");

    // 3. Create document to test Optimistic Concurrency Control (OCC)
    let res_create = fixture
        .server
        .post("/api/v1/content/articles")
        .add_header("authorization", &auth)
        .json(&json!({
            "fields": {
                "title": "OCC Article"
            }
        }))
        .await;
    res_create.assert_status(StatusCode::CREATED);
    let doc_id = res_create.json::<Value>()["data"]["id"]
        .as_str()
        .expect("doc id")
        .to_string();

    // 4. Updating with mismatched expectedVersion -> 409 version_mismatch
    let res_occ = fixture
        .server
        .put(&format!("/api/v1/content/articles/{doc_id}"))
        .add_header("authorization", &auth)
        .json(&json!({
            "fields": {
                "title": "Mismatched Update"
            },
            "expectedVersion": 999
        }))
        .await;
    res_occ.assert_status_conflict();
    let occ_body: Value = res_occ.json();
    assert_eq!(occ_body["code"], "version_mismatch");
    assert_eq!(occ_body["details"]["expected_version"], 999);
    assert_eq!(occ_body["details"]["found_version"], 1);
}

#[tokio::test]
async fn test_07_content_lifecycle_and_snapshots() {
    let fixture = TestFixture::new().await;
    let auth = fixture.admin_bearer();

    // Step 1: Create Draft document
    let doc_id = assert_draft_creation_and_visibility(&fixture, &auth).await;

    // Step 2: Publish revision 1 and update to Modified
    assert_first_publish_and_update(&fixture, &auth, &doc_id).await;

    // Step 3: Publish revision 2, creating revision 1 snapshot
    assert_second_publish_and_snapshots(&fixture, &auth, &doc_id).await;

    // Step 4: Test restore conflict guard and successful snapshot recovery
    assert_restore_conflict_and_recovery(&fixture, &auth, &doc_id).await;

    // Step 5: Test draft discarding, unpublishing, and final deletion
    assert_discard_unpublish_and_delete(&fixture, &auth, &doc_id).await;
}

// ---------------------------------------------------------------------------
// Private Lifecycle Sub-Step Helpers (SLAP & SRP Decomposition)
// ---------------------------------------------------------------------------

async fn assert_draft_creation_and_visibility(fixture: &TestFixture, auth: &str) -> String {
    // 1. Create Draft
    let res_create = fixture
        .server
        .post("/api/v1/content/articles")
        .add_header("authorization", auth)
        .json(&json!({
            "fields": {
                "title": "Lifecycle Article V1",
                "content": "Initial content"
            }
        }))
        .await;
    res_create.assert_status(StatusCode::CREATED);
    let create_body: Value = res_create.json();
    let doc_id = create_body["data"]["id"]
        .as_str()
        .expect("doc id")
        .to_string();
    assert_eq!(create_body["data"]["publicationState"]["status"], "draft");
    assert_eq!(create_body["data"]["audit"]["version"], 1);

    // 2. Default view=published returns 0 items while still draft
    let res_pub_list = fixture
        .server
        .get("/api/v1/content/articles")
        .add_header("authorization", auth)
        .await;
    res_pub_list.assert_status_ok();
    assert_eq!(res_pub_list.json::<Value>()["meta"]["total"], 0);

    // 3. view=working returns 1 item
    let res_working_list = fixture
        .server
        .get("/api/v1/content/articles?view=working")
        .add_header("authorization", auth)
        .await;
    res_working_list.assert_status_ok();
    assert_eq!(res_working_list.json::<Value>()["meta"]["total"], 1);

    // 4. Admin headers returns 1 header with status draft
    let res_headers = fixture
        .server
        .get("/api/v1/content/articles/admin/headers")
        .add_header("authorization", auth)
        .await;
    res_headers.assert_status_ok();
    let headers_body: Value = res_headers.json();
    assert_eq!(headers_body["meta"]["total"], 1);
    assert_eq!(headers_body["data"][0]["status"], "draft");

    doc_id
}

async fn assert_first_publish_and_update(fixture: &TestFixture, auth: &str, doc_id: &str) {
    // 1. POST .../publish -> revision 1
    let res_pub = fixture
        .server
        .post(&format!("/api/v1/content/articles/{doc_id}/publish"))
        .add_header("authorization", auth)
        .await;
    res_pub.assert_status_ok();
    let pub_body: Value = res_pub.json();
    assert_eq!(pub_body["data"]["publicationState"]["status"], "published");
    assert_eq!(pub_body["data"]["publicationState"]["revision"], 1);

    // 2. GET /articles/{id} (default view=published) returns live document
    let res_get = fixture
        .server
        .get(&format!("/api/v1/content/articles/{doc_id}"))
        .add_header("authorization", auth)
        .await;
    res_get.assert_status_ok();
    assert_eq!(
        res_get.json::<Value>()["data"]["fields"]["title"],
        "Lifecycle Article V1"
    );

    // 3. PUT .../articles/{id} updates fields -> transitions to Modified
    let res_put = fixture
        .server
        .put(&format!("/api/v1/content/articles/{doc_id}"))
        .add_header("authorization", auth)
        .json(&json!({
            "fields": {
                "title": "Lifecycle Article V2",
                "content": "Updated content V2"
            }
        }))
        .await;
    res_put.assert_status_ok();
    let put_body: Value = res_put.json();
    assert_eq!(put_body["data"]["publicationState"]["status"], "modified");
    assert_eq!(put_body["data"]["publicationState"]["liveRevision"], 1);
    assert_eq!(put_body["data"]["publicationState"]["draftVersion"], 3);
}

async fn assert_second_publish_and_snapshots(fixture: &TestFixture, auth: &str, doc_id: &str) {
    // 1. Publish revision 2 (archives revision 1 as a historical snapshot)
    let res_pub2 = fixture
        .server
        .post(&format!("/api/v1/content/articles/{doc_id}/publish"))
        .add_header("authorization", auth)
        .await;
    res_pub2.assert_status_ok();
    assert_eq!(
        res_pub2.json::<Value>()["data"]["publicationState"]["revision"],
        2
    );

    // 2. GET .../snapshots lists revision 1 snapshot header
    let res_snaps = fixture
        .server
        .get(&format!("/api/v1/content/articles/{doc_id}/snapshots"))
        .add_header("authorization", auth)
        .await;
    res_snaps.assert_status_ok();
    let snaps_body: Value = res_snaps.json();
    let snaps = snaps_body["data"].as_array().expect("snapshots array");
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0]["revision"], 1);

    // 3. GET .../snapshots/1 returns full historical snapshot content
    let res_snap1 = fixture
        .server
        .get(&format!("/api/v1/content/articles/{doc_id}/snapshots/1"))
        .add_header("authorization", auth)
        .await;
    res_snap1.assert_status_ok();
    let snap1_body: Value = res_snap1.json();
    assert_eq!(snap1_body["data"]["revision"], 1);
    assert_eq!(
        snap1_body["data"]["fields"]["title"],
        "Lifecycle Article V1"
    );
}

async fn assert_restore_conflict_and_recovery(fixture: &TestFixture, auth: &str, doc_id: &str) {
    // 1. Edit working draft to put document into Modified state
    let res_edit = fixture
        .server
        .put(&format!("/api/v1/content/articles/{doc_id}"))
        .add_header("authorization", auth)
        .json(&json!({
            "fields": {
                "title": "Pending Edits V3"
            }
        }))
        .await;
    res_edit.assert_status_ok();

    // 2. Attempt restoring revision 1 without discardPendingEdits -> 409 Conflict
    let restore_url = format!("/api/v1/content/articles/{doc_id}/snapshots/1/restore");
    let res_conflict = fixture
        .server
        .post(&restore_url)
        .add_header("authorization", auth)
        .json(&json!({}))
        .await;
    res_conflict.assert_status_conflict();
    let conflict_body: Value = res_conflict.json();
    assert_eq!(conflict_body["code"], "draft_has_pending_edits");
    assert_eq!(conflict_body["instance"], restore_url);

    // 3. Restore revision 1 with discardPendingEdits: true -> 200 OK
    let res_restored = fixture
        .server
        .post(&restore_url)
        .add_header("authorization", auth)
        .json(&json!({"discardPendingEdits": true}))
        .await;
    res_restored.assert_status_ok();
    let restored_body: Value = res_restored.json();
    assert_eq!(
        restored_body["data"]["fields"]["title"],
        "Lifecycle Article V1"
    );
}

async fn assert_discard_unpublish_and_delete(fixture: &TestFixture, auth: &str, doc_id: &str) {
    // 1. Discard pending draft edits -> reverts to clean Published revision 2
    let res_discard = fixture
        .server
        .post(&format!("/api/v1/content/articles/{doc_id}/discard-draft"))
        .add_header("authorization", auth)
        .await;
    res_discard.assert_status_ok();
    let discard_body: Value = res_discard.json();
    assert_eq!(
        discard_body["data"]["publicationState"]["status"],
        "published"
    );
    assert_eq!(
        discard_body["data"]["fields"]["title"],
        "Lifecycle Article V2"
    );

    // 2. Unpublish document -> transitions to Unpublished
    let res_unpub = fixture
        .server
        .post(&format!("/api/v1/content/articles/{doc_id}/unpublish"))
        .add_header("authorization", auth)
        .await;
    res_unpub.assert_status_ok();
    let unpub_body: Value = res_unpub.json();
    assert_eq!(
        unpub_body["data"]["publicationState"]["status"],
        "unpublished"
    );

    // 3. Delete document -> 204 No Content
    let res_del = fixture
        .server
        .delete(&format!("/api/v1/content/articles/{doc_id}"))
        .add_header("authorization", auth)
        .await;
    res_del.assert_status_no_content();

    // 4. Verify document is not found after deletion
    let res_gone = fixture
        .server
        .get(&format!("/api/v1/content/articles/{doc_id}?view=working"))
        .add_header("authorization", auth)
        .await;
    res_gone.assert_status_not_found();
}
