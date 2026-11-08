//! End-to-end integration test suite exercising the full container topology.
//!
//! Validates health probes, Dex OIDC authentication, onboarding lifecycle,
//! single-lifecycle document types, singleton uniqueness and lifecycle,
//! relational mutations, optimistic concurrency control, and revision snapshots
//! against a live container deployment.

mod e2e_support;

use reqwest::StatusCode;
use serde_json::json;

use infrastructure::persistence::migration::{ROLE_ADMIN_ID, ROLE_EDITOR_ID};

use e2e_support::{
    E2eClient, E2eEnvironment, assert_json_include, assert_problem_details, assert_status,
    extract_data_id, parse_json_with_status,
};

// ---------------------------------------------------------------------------
// Context State Structs
// ---------------------------------------------------------------------------

/// Authenticated OIDC ID tokens for onboarded users.
struct OnboardedTokens {
    admin_jwt: String,
    editor_jwt: String,
}

/// Identifiers of seeded category and tag documents for relation linking.
struct SeededTaxonomies {
    cat_tech_id: String,
    cat_sci_id: String,
    tag_rust_id: String,
    tag_docker_id: String,
}

// ---------------------------------------------------------------------------
// Top-Level Test Entry Point (Newspaper Style)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_e2e_full_service_lifecycle() {
    if !is_docker_available() {
        eprintln!("Skipping test_e2e_full_service_lifecycle: Docker daemon is not accessible");
        return;
    }

    let env = E2eEnvironment::start().await;
    let tokens = verify_health_and_onboarding(&env.client).await;
    let taxonomies = verify_single_lifecycle_collections(&env.client, &tokens).await;
    verify_singleton_lifecycle(&env.client, &tokens).await;
    verify_article_lifecycle_and_relations(&env.client, &tokens, &taxonomies).await;
}

// ---------------------------------------------------------------------------
// Phase 3.1: Health & Onboarding Orchestrator
// ---------------------------------------------------------------------------

/// Executes Step 3.1: Health probes, live Dex RS256 token verification,
/// profile synchronization, access requests, M2M bootstrap approval, and directory inspection.
async fn verify_health_and_onboarding(client: &E2eClient) -> OnboardedTokens {
    verify_health_and_readiness_probes(client).await;
    let (admin_jwt, editor_jwt) = acquire_tokens_and_verify_signature(client).await;
    verify_pre_onboarding_guard(client, &admin_jwt).await;

    let (admin_req_id, editor_req_id) =
        submit_onboarding_profiles(client, &admin_jwt, &editor_jwt).await;
    verify_pending_access_guard(client, &admin_jwt).await;

    approve_onboarding_requests(client, &admin_jwt, &admin_req_id, &editor_req_id).await;
    verify_system_and_directory(client, &admin_jwt, &editor_jwt).await;

    OnboardedTokens {
        admin_jwt,
        editor_jwt,
    }
}

// ---------------------------------------------------------------------------
// Phase 3.2: Single-Lifecycle Collections Orchestrator
// ---------------------------------------------------------------------------

/// Executes Step 3.2: Single-lifecycle document types (`draftAndPublish: false`),
/// immediate publication verification, lifecycle guards, and CRUD mutations.
async fn verify_single_lifecycle_collections(
    client: &E2eClient,
    tokens: &OnboardedTokens,
) -> SeededTaxonomies {
    let (cat_tech_id, cat_sci_id) = create_category_taxonomies(client, &tokens.editor_jwt).await;
    let (tag_rust_id, tag_docker_id, tag_obsolete_id) =
        create_tag_taxonomies(client, &tokens.editor_jwt).await;

    verify_taxonomies_collection_totals(client, &tokens.editor_jwt).await;
    verify_draft_disabled_lifecycle_guards(client, &tokens.editor_jwt, &cat_tech_id).await;
    verify_single_lifecycle_mutation_and_deletion(
        client,
        &tokens.editor_jwt,
        &tag_obsolete_id,
    )
    .await;

    SeededTaxonomies {
        cat_tech_id,
        cat_sci_id,
        tag_rust_id,
        tag_docker_id,
    }
}

// ---------------------------------------------------------------------------
// Phase 4.1: Singleton Lifecycle Orchestrator
// ---------------------------------------------------------------------------

/// Executes Step 4.1: Singleton document lifecycle (`site-settings`, `kind: "singleton"`),
/// uniqueness guard, publish revision 1, draft edits in modified state,
/// published vs working view isolation, and revision 2 publication.
async fn verify_singleton_lifecycle(client: &E2eClient, tokens: &OnboardedTokens) {
    let settings_id = create_singleton_draft(client, &tokens.admin_jwt).await;
    verify_singleton_uniqueness_guard(client, &tokens.admin_jwt).await;
    verify_singleton_publish_and_modify(client, &tokens.admin_jwt, &settings_id).await;
    verify_singleton_views_and_second_publish(client, &tokens.admin_jwt, &settings_id).await;
}

// ---------------------------------------------------------------------------
// Phase 4.2: Full Collection Lifecycle, Relations & Snapshots Orchestrator
// ---------------------------------------------------------------------------

/// Executes Step 4.2: Full collection lifecycle (`article`, `draftAndPublish: true`),
/// exercising all field types, draft view isolation, relation population,
/// OCC version conflicts, relational mutations (set/connect/disconnect),
/// revision snapshots, conflict-guarded restore, discard draft, unpublish, and deletion.
async fn verify_article_lifecycle_and_relations(
    client: &E2eClient,
    tokens: &OnboardedTokens,
    taxonomies: &SeededTaxonomies,
) {
    let (art_1_id, art_2_id) =
        create_article_drafts(client, &tokens.editor_jwt, taxonomies).await;
    verify_article_draft_views_and_headers(client, &tokens.editor_jwt).await;
    verify_article_first_publish_and_populate(client, &tokens.editor_jwt, &art_1_id).await;
    verify_article_occ_and_relation_mutations(
        client,
        &tokens.editor_jwt,
        &art_1_id,
        taxonomies,
    )
    .await;
    verify_article_snapshots_and_restore(
        client,
        &tokens.editor_jwt,
        &art_1_id,
        taxonomies,
    )
    .await;
    verify_article_discard_unpublish_and_delete(
        client,
        &tokens.editor_jwt,
        &art_1_id,
        &art_2_id,
    )
    .await;
}

fn is_docker_available() -> bool {
    std::process::Command::new("docker")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Phase 3.1 Leaf Helpers (Workers)
// ---------------------------------------------------------------------------

async fn verify_health_and_readiness_probes(client: &E2eClient) {
    let healthz_body = parse_json_with_status(
        client.get_unauthenticated("/healthz").await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &healthz_body,
        expected: json!({"status": "healthy"})
    );

    let ready_body = parse_json_with_status(
        client.get_unauthenticated("/ready").await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &ready_body,
        expected: json!({"status": "healthy"})
    );
}

async fn acquire_tokens_and_verify_signature(client: &E2eClient) -> (String, String) {
    let admin_jwt = client
        .fetch_dex_id_token("admin@luminair.dev", "password")
        .await;
    let editor_jwt = client
        .fetch_dex_id_token("editor@luminair.dev", "password")
        .await;

    let forged_resp = client.get("/api/v1/system/config", "invalid.jwt.token").await;
    assert_problem_details(forged_resp, StatusCode::UNAUTHORIZED, "invalid_token").await;

    (admin_jwt, editor_jwt)
}

async fn verify_pre_onboarding_guard(client: &E2eClient, admin_jwt: &str) {
    let resp = client.get("/api/v1/system/config", admin_jwt).await;
    assert_problem_details(resp, StatusCode::FORBIDDEN, "access_not_requested").await;
}

async fn submit_onboarding_profiles(
    client: &E2eClient,
    admin_jwt: &str,
    editor_jwt: &str,
) -> (String, String) {
    sync_user_profile(client, admin_jwt, "admin@luminair.dev").await;
    sync_user_profile(client, editor_jwt, "editor@luminair.dev").await;

    let admin_req_id = submit_access_request(client, admin_jwt).await;
    let editor_req_id = submit_access_request(client, editor_jwt).await;

    (admin_req_id, editor_req_id)
}

async fn sync_user_profile(client: &E2eClient, token: &str, expected_email: &str) {
    let resp = client
        .post_json("/api/v1/auth/profile/sync", token, &json!({}))
        .await;
    let body = parse_json_with_status(resp, StatusCode::OK).await;
    assert_eq!(body["data"]["email"], expected_email);
}

async fn submit_access_request(client: &E2eClient, token: &str) -> String {
    let resp = client.post_empty("/api/v1/auth/access-requests", token).await;
    let body = parse_json_with_status(resp, StatusCode::CREATED).await;
    extract_data_id(&body)
}

async fn verify_pending_access_guard(client: &E2eClient, admin_jwt: &str) {
    let resp = client.get("/api/v1/system/config", admin_jwt).await;
    assert_problem_details(resp, StatusCode::FORBIDDEN, "access_pending").await;
}

async fn approve_onboarding_requests(
    client: &E2eClient,
    admin_jwt: &str,
    admin_req_id: &str,
    editor_req_id: &str,
) {
    approve_and_verify_status(
        client,
        &client.service_token,
        admin_req_id,
        ROLE_ADMIN_ID,
        &client.service_token,
    )
    .await;

    approve_and_verify_status(
        client,
        admin_jwt,
        editor_req_id,
        ROLE_EDITOR_ID,
        admin_jwt,
    )
    .await;
}

async fn approve_and_verify_status(
    client: &E2eClient,
    approver_token: &str,
    req_id: &str,
    role_id: uuid::Uuid,
    viewer_token: &str,
) {
    let approve_path = format!("/api/v1/auth/access-requests/{req_id}/approve");
    let resp = client
        .post_json(&approve_path, approver_token, &json!({ "roleIds": [role_id] }))
        .await;
    assert_status(resp.status(), StatusCode::OK);

    let get_path = format!("/api/v1/auth/access-requests/{req_id}");
    let get_body =
        parse_json_with_status(client.get(&get_path, viewer_token).await, StatusCode::OK).await;
    assert_eq!(get_body["data"]["status"]["status"], "approved");
}

async fn verify_system_and_directory(
    client: &E2eClient,
    admin_jwt: &str,
    editor_jwt: &str,
) {
    verify_system_config_and_locales(client, admin_jwt, editor_jwt).await;
    verify_schemas_and_user_directory(client, admin_jwt, editor_jwt).await;
}

async fn verify_system_config_and_locales(
    client: &E2eClient,
    admin_jwt: &str,
    editor_jwt: &str,
) {
    let config_body = parse_json_with_status(
        client.get("/api/v1/system/config", admin_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &config_body,
        expected: json!({
            "data": {
                "defaultLocale": "en",
                "availableLocales": ["en", "uk", "fr", "de"]
            }
        })
    );

    let locales_body = parse_json_with_status(
        client.get("/api/v1/system/locales", editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &locales_body,
        expected: json!({
            "data": { "defaultLocale": "en" }
        })
    );
}

async fn verify_schemas_and_user_directory(
    client: &E2eClient,
    admin_jwt: &str,
    editor_jwt: &str,
) {
    let schemas_body = parse_json_with_status(
        client.get("/api/v1/schema/document-types", editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    let schemas_len = schemas_body["data"]
        .as_array()
        .expect("schemas array")
        .len();
    assert_eq!(schemas_len, 4);

    let users_body = parse_json_with_status(
        client.get("/api/v1/auth/users?q=admin", admin_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(users_body["meta"]["total"], 1);
    assert_eq!(users_body["data"][0]["email"], "admin@luminair.dev");
}

// ---------------------------------------------------------------------------
// Phase 3.2 Leaf Helpers (Workers)
// ---------------------------------------------------------------------------

async fn create_category_taxonomies(
    client: &E2eClient,
    editor_jwt: &str,
) -> (String, String) {
    let cat_tech_id = create_tech_category(client, editor_jwt).await;
    let cat_sci_id = create_sci_category(client, editor_jwt).await;
    (cat_tech_id, cat_sci_id)
}

async fn create_tech_category(client: &E2eClient, editor_jwt: &str) -> String {
    let body = parse_json_with_status(
        client
            .post_json(
                "/api/v1/content/category",
                editor_jwt,
                &json!({ "fields": { "name": "Technology" } }),
            )
            .await,
        StatusCode::CREATED,
    )
    .await;
    assert_json_include!(
        actual: &body,
        expected: json!({
            "data": {
                "documentTypeId": "category",
                "fields": { "name": "Technology" },
                "publicationState": {
                    "status": "published",
                    "revision": 1
                }
            }
        })
    );
    extract_data_id(&body)
}

async fn create_sci_category(client: &E2eClient, editor_jwt: &str) -> String {
    let body = parse_json_with_status(
        client
            .post_json(
                "/api/v1/content/category",
                editor_jwt,
                &json!({ "fields": { "name": "Science" } }),
            )
            .await,
        StatusCode::CREATED,
    )
    .await;
    assert_json_include!(
        actual: &body,
        expected: json!({
            "data": {
                "publicationState": { "status": "published" }
            }
        })
    );
    extract_data_id(&body)
}

async fn create_tag_taxonomies(
    client: &E2eClient,
    editor_jwt: &str,
) -> (String, String, String) {
    let tag_rust_id = create_single_tag(client, editor_jwt, "rust").await;
    let tag_docker_id = create_single_tag(client, editor_jwt, "docker").await;
    let tag_obsolete_id = create_single_tag(client, editor_jwt, "obsolete").await;

    (tag_rust_id, tag_docker_id, tag_obsolete_id)
}

async fn create_single_tag(client: &E2eClient, editor_jwt: &str, name: &str) -> String {
    let body = parse_json_with_status(
        client
            .post_json(
                "/api/v1/content/tag",
                editor_jwt,
                &json!({ "fields": { "name": name } }),
            )
            .await,
        StatusCode::CREATED,
    )
    .await;
    extract_data_id(&body)
}

async fn verify_taxonomies_collection_totals(client: &E2eClient, editor_jwt: &str) {
    let cat_body = parse_json_with_status(
        client.get("/api/v1/content/category", editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(cat_body["meta"]["total"], 2);

    let tag_body = parse_json_with_status(
        client.get("/api/v1/content/tag", editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(tag_body["meta"]["total"], 3);
}

async fn verify_draft_disabled_lifecycle_guards(
    client: &E2eClient,
    editor_jwt: &str,
    cat_tech_id: &str,
) {
    let publish_path = format!("/api/v1/content/category/{cat_tech_id}/publish");
    let pub_resp = client.post_empty(&publish_path, editor_jwt).await;
    assert_problem_details(
        pub_resp,
        StatusCode::CONFLICT,
        "draft_and_publish_disabled",
    )
    .await;

    let unpublish_path = format!("/api/v1/content/category/{cat_tech_id}/unpublish");
    let unpub_resp = client.post_empty(&unpublish_path, editor_jwt).await;
    assert_problem_details(
        unpub_resp,
        StatusCode::CONFLICT,
        "draft_and_publish_disabled",
    )
    .await;
}

async fn verify_single_lifecycle_mutation_and_deletion(
    client: &E2eClient,
    editor_jwt: &str,
    tag_obsolete_id: &str,
) {
    let tag_path = format!("/api/v1/content/tag/{tag_obsolete_id}");
    let update_body = parse_json_with_status(
        client
            .put_json(
                &tag_path,
                editor_jwt,
                &json!({ "fields": { "name": "deprecated" } }),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(update_body["data"]["fields"]["name"], "deprecated");
    assert_eq!(update_body["data"]["audit"]["version"], 2);

    let del_resp = client.delete(&tag_path, editor_jwt).await;
    assert_status(del_resp.status(), StatusCode::NO_CONTENT);

    let get_resp = client.get(&tag_path, editor_jwt).await;
    assert_status(get_resp.status(), StatusCode::NOT_FOUND);

    let list_body = parse_json_with_status(
        client.get("/api/v1/content/tag", editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(list_body["meta"]["total"], 2);
}

// ---------------------------------------------------------------------------
// Phase 4.1 Leaf Helpers (Workers)
// ---------------------------------------------------------------------------

async fn create_singleton_draft(client: &E2eClient, admin_jwt: &str) -> String {
    let payload = json!({
        "fields": {
            "site-name": "Luminair Cloud",
            "maintenance-mode": false
        }
    });
    let body = parse_json_with_status(
        client
            .post_json("/api/v1/content/site-settings", admin_jwt, &payload)
            .await,
        StatusCode::CREATED,
    )
    .await;
    assert_json_include!(
        actual: &body,
        expected: json!({
            "data": {
                "documentTypeId": "site-settings",
                "publicationState": { "status": "draft" },
                "fields": {
                    "site-name": "Luminair Cloud",
                    "maintenance-mode": false
                }
            }
        })
    );
    extract_data_id(&body)
}

async fn verify_singleton_uniqueness_guard(client: &E2eClient, admin_jwt: &str) {
    let dup_payload = json!({
        "fields": {
            "site-name": "Duplicate Site"
        }
    });
    let resp = client
        .post_json("/api/v1/content/site-settings", admin_jwt, &dup_payload)
        .await;
    assert_problem_details(resp, StatusCode::CONFLICT, "singleton_already_exists").await;
}

async fn verify_singleton_publish_and_modify(
    client: &E2eClient,
    admin_jwt: &str,
    settings_id: &str,
) {
    verify_singleton_first_publish(client, admin_jwt, settings_id).await;
    modify_singleton_draft(client, admin_jwt, settings_id).await;
}

async fn verify_singleton_first_publish(
    client: &E2eClient,
    admin_jwt: &str,
    settings_id: &str,
) {
    let pub_path = format!("/api/v1/content/site-settings/{settings_id}/publish");
    let pub_body = parse_json_with_status(
        client.post_empty(&pub_path, admin_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &pub_body,
        expected: json!({
            "data": {
                "publicationState": {
                    "status": "published",
                    "revision": 1
                }
            }
        })
    );
}

async fn modify_singleton_draft(
    client: &E2eClient,
    admin_jwt: &str,
    settings_id: &str,
) {
    let put_path = format!("/api/v1/content/site-settings/{settings_id}");
    let put_body = parse_json_with_status(
        client
            .put_json(
                &put_path,
                admin_jwt,
                &json!({
                    "fields": {
                        "site-name": "Luminair Cloud V2",
                        "maintenance-mode": true
                    }
                }),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &put_body,
        expected: json!({
            "data": {
                "publicationState": {
                    "status": "modified",
                    "liveRevision": 1,
                    "draftVersion": 3
                }
            }
        })
    );
}

async fn verify_singleton_views_and_second_publish(
    client: &E2eClient,
    admin_jwt: &str,
    settings_id: &str,
) {
    verify_singleton_views_isolation(client, admin_jwt, settings_id).await;
    verify_singleton_second_publish(client, admin_jwt, settings_id).await;
}

async fn verify_singleton_views_isolation(
    client: &E2eClient,
    admin_jwt: &str,
    settings_id: &str,
) {
    let get_path = format!("/api/v1/content/site-settings/{settings_id}");
    let pub_view =
        parse_json_with_status(client.get(&get_path, admin_jwt).await, StatusCode::OK).await;
    assert_json_include!(
        actual: &pub_view,
        expected: json!({
            "data": {
                "fields": {
                    "site-name": "Luminair Cloud",
                    "maintenance-mode": false
                }
            }
        })
    );

    let working_path = format!("{get_path}?view=working");
    let work_view =
        parse_json_with_status(client.get(&working_path, admin_jwt).await, StatusCode::OK).await;
    assert_json_include!(
        actual: &work_view,
        expected: json!({
            "data": {
                "fields": {
                    "site-name": "Luminair Cloud V2",
                    "maintenance-mode": true
                }
            }
        })
    );
}

async fn verify_singleton_second_publish(
    client: &E2eClient,
    admin_jwt: &str,
    settings_id: &str,
) {
    let pub2_path = format!("/api/v1/content/site-settings/{settings_id}/publish");
    let pub2_body = parse_json_with_status(
        client.post_empty(&pub2_path, admin_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &pub2_body,
        expected: json!({
            "data": {
                "publicationState": {
                    "status": "published",
                    "revision": 2
                }
            }
        })
    );
}

// ---------------------------------------------------------------------------
// Phase 4.2 Leaf Helpers (Workers)
// ---------------------------------------------------------------------------

async fn create_article_drafts(
    client: &E2eClient,
    editor_jwt: &str,
    taxonomies: &SeededTaxonomies,
) -> (String, String) {
    let art_1_id = create_first_article_draft(client, editor_jwt, taxonomies).await;
    let art_2_id = create_second_article_draft(client, editor_jwt, taxonomies).await;
    (art_1_id, art_2_id)
}

async fn create_first_article_draft(
    client: &E2eClient,
    editor_jwt: &str,
    taxonomies: &SeededTaxonomies,
) -> String {
    let payload = first_article_payload(taxonomies);
    let body = parse_json_with_status(
        client
            .post_json("/api/v1/content/article", editor_jwt, &payload)
            .await,
        StatusCode::CREATED,
    )
    .await;
    assert_json_include!(
        actual: &body,
        expected: json!({
            "data": {
                "documentTypeId": "article",
                "publicationState": { "status": "draft" },
                "fields": { "price": "19.99" },
                "relations": {
                    "category": [&taxonomies.cat_tech_id],
                    "tags": [&taxonomies.tag_rust_id]
                }
            }
        })
    );
    extract_data_id(&body)
}

fn first_article_payload(taxonomies: &SeededTaxonomies) -> serde_json::Value {
    json!({
        "fields": {
            "title": "Rust Testcontainers Architecture",
            "slug": "rust-testcontainers-arch",
            "summary": {
                "en": "Comprehensive guide to E2E testing with Testcontainers in Rust.",
                "uk": "Повний посібник з E2E тестування за допомогою Testcontainers у Rust."
            },
            "body": "Initial article body v1",
            "views": 100,
            "priority": 1,
            "price": "19.99",
            "metadata": {
                "featured": true,
                "readingTime": 5
            }
        },
        "relations": {
            "category": [&taxonomies.cat_tech_id],
            "tags": [&taxonomies.tag_rust_id]
        }
    })
}

async fn create_second_article_draft(
    client: &E2eClient,
    editor_jwt: &str,
    taxonomies: &SeededTaxonomies,
) -> String {
    let payload = json!({
        "fields": {
            "title": "Docker Bridge Networking",
            "slug": "docker-bridge-networking",
            "summary": {
                "en": "Deep dive into isolated Docker networks for microservices."
            },
            "priority": 2
        },
        "relations": {
            "category": [&taxonomies.cat_sci_id],
            "tags": [&taxonomies.tag_rust_id, &taxonomies.tag_docker_id]
        }
    });
    let body = parse_json_with_status(
        client
            .post_json("/api/v1/content/article", editor_jwt, &payload)
            .await,
        StatusCode::CREATED,
    )
    .await;
    assert_json_include!(
        actual: &body,
        expected: json!({
            "data": {
                "documentTypeId": "article",
                "publicationState": { "status": "draft" }
            }
        })
    );
    extract_data_id(&body)
}

async fn verify_article_draft_views_and_headers(client: &E2eClient, editor_jwt: &str) {
    let pub_body = parse_json_with_status(
        client.get("/api/v1/content/article", editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(pub_body["meta"]["total"], 0);

    let draft_body = parse_json_with_status(
        client
            .get("/api/v1/content/article?view=working", editor_jwt)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(draft_body["meta"]["total"], 2);

    let headers_body = parse_json_with_status(
        client
            .get("/api/v1/content/article/admin/headers", editor_jwt)
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(headers_body["meta"]["total"], 2);
    let headers = headers_body["data"].as_array().expect("headers array");
    assert_eq!(headers.len(), 2);
    assert!(headers.iter().all(|h| h["status"] == "draft"));
}

async fn verify_article_first_publish_and_populate(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
) {
    let pub_path = format!("/api/v1/content/article/{art_1_id}/publish");
    let pub_body = parse_json_with_status(
        client.post_empty(&pub_path, editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &pub_body,
        expected: json!({
            "data": {
                "publicationState": {
                    "status": "published",
                    "revision": 1
                }
            }
        })
    );

    let dup_resp = client.post_empty(&pub_path, editor_jwt).await;
    assert_problem_details(dup_resp, StatusCode::CONFLICT, "already_published").await;

    let pop_path = format!("/api/v1/content/article/{art_1_id}?populate=category,tags");
    let pop_body =
        parse_json_with_status(client.get(&pop_path, editor_jwt).await, StatusCode::OK).await;
    assert_eq!(
        pop_body["data"]["populatedRelations"]["category"][0]["fields"]["name"],
        "Technology"
    );
    assert_eq!(
        pop_body["data"]["populatedRelations"]["tags"][0]["fields"]["name"],
        "rust"
    );
}

async fn verify_article_occ_and_relation_mutations(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
    taxonomies: &SeededTaxonomies,
) {
    verify_article_occ_conflict(client, editor_jwt, art_1_id).await;
    verify_article_relation_set_and_connect(client, editor_jwt, art_1_id, taxonomies).await;
    verify_article_relation_disconnect(client, editor_jwt, art_1_id, taxonomies).await;
}

async fn verify_article_occ_conflict(client: &E2eClient, editor_jwt: &str, art_1_id: &str) {
    let path = format!("/api/v1/content/article/{art_1_id}");
    let stale_payload = json!({
        "fields": {
            "title": "Stale Edit",
            "slug": "rust-testcontainers-arch",
            "summary": { "en": "Valid length summary text" },
            "priority": 1
        },
        "expectedVersion": 999
    });
    let resp = client.put_json(&path, editor_jwt, &stale_payload).await;
    assert_problem_details(resp, StatusCode::CONFLICT, "version_mismatch").await;
}

async fn verify_article_relation_set_and_connect(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
    taxonomies: &SeededTaxonomies,
) {
    let path = format!("/api/v1/content/article/{art_1_id}");
    let payload = article_v2_set_connect_payload(taxonomies);
    let body = parse_json_with_status(
        client.put_json(&path, editor_jwt, &payload).await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &body,
        expected: json!({
            "data": {
                "publicationState": {
                    "status": "modified",
                    "liveRevision": 1
                },
                "relations": {
                    "category": [&taxonomies.cat_sci_id]
                }
            }
        })
    );
    let tags = body["data"]["relations"]["tags"]
        .as_array()
        .expect("tags array");
    assert_eq!(tags.len(), 2);
    assert!(tags.iter().any(|t| t == &taxonomies.tag_rust_id));
    assert!(tags.iter().any(|t| t == &taxonomies.tag_docker_id));
}

fn article_v2_set_connect_payload(taxonomies: &SeededTaxonomies) -> serde_json::Value {
    json!({
        "fields": {
            "title": "Rust Testcontainers Architecture V2",
            "slug": "rust-testcontainers-arch",
            "summary": {
                "en": "Updated comprehensive guide to E2E testing with Testcontainers."
            },
            "body": "Updated article body v2",
            "views": 250,
            "priority": 1,
            "price": "29.99"
        },
        "relations": {
            "category": { "set": [&taxonomies.cat_sci_id] },
            "tags": { "connect": [&taxonomies.tag_docker_id] }
        }
    })
}

async fn verify_article_relation_disconnect(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
    taxonomies: &SeededTaxonomies,
) {
    let path = format!("/api/v1/content/article/{art_1_id}");
    let payload = article_v2_disconnect_payload(taxonomies);
    let body = parse_json_with_status(
        client.put_json(&path, editor_jwt, &payload).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        body["data"]["relations"]["tags"],
        json!([&taxonomies.tag_docker_id])
    );
}

fn article_v2_disconnect_payload(taxonomies: &SeededTaxonomies) -> serde_json::Value {
    json!({
        "fields": {
            "title": "Rust Testcontainers Architecture V2",
            "slug": "rust-testcontainers-arch",
            "summary": {
                "en": "Updated comprehensive guide to E2E testing with Testcontainers."
            },
            "body": "Updated article body v2",
            "views": 250,
            "priority": 1,
            "price": "29.99"
        },
        "relations": {
            "tags": { "disconnect": [&taxonomies.tag_rust_id] }
        }
    })
}

async fn verify_article_snapshots_and_restore(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
    taxonomies: &SeededTaxonomies,
) {
    publish_article_revision_2(client, editor_jwt, art_1_id).await;
    verify_historical_snapshot_contents(client, editor_jwt, art_1_id, taxonomies).await;
    verify_snapshot_restore_conflict_and_recovery(client, editor_jwt, art_1_id, taxonomies).await;
}

async fn publish_article_revision_2(client: &E2eClient, editor_jwt: &str, art_1_id: &str) {
    let pub_path = format!("/api/v1/content/article/{art_1_id}/publish");
    let pub_body = parse_json_with_status(
        client.post_empty(&pub_path, editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(pub_body["data"]["publicationState"]["revision"], 2);
}

async fn verify_historical_snapshot_contents(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
    taxonomies: &SeededTaxonomies,
) {
    let list_path = format!("/api/v1/content/article/{art_1_id}/snapshots");
    let list_body =
        parse_json_with_status(client.get(&list_path, editor_jwt).await, StatusCode::OK).await;
    let snaps = list_body["data"].as_array().expect("snapshots array");
    assert_eq!(snaps.len(), 1);
    assert_eq!(snaps[0]["revision"], 1);

    let get_path = format!("/api/v1/content/article/{art_1_id}/snapshots/1");
    let snap_body =
        parse_json_with_status(client.get(&get_path, editor_jwt).await, StatusCode::OK).await;
    assert_json_include!(
        actual: &snap_body,
        expected: json!({
            "data": {
                "fields": {
                    "title": "Rust Testcontainers Architecture",
                    "price": "19.99"
                },
                "relations": {
                    "category": [&taxonomies.cat_tech_id],
                    "tags": [&taxonomies.tag_rust_id]
                }
            }
        })
    );
}

async fn verify_snapshot_restore_conflict_and_recovery(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
    taxonomies: &SeededTaxonomies,
) {
    create_dirty_article_draft(client, editor_jwt, art_1_id).await;
    assert_snapshot_restore_conflict(client, editor_jwt, art_1_id).await;
    execute_snapshot_restore_with_discard(client, editor_jwt, art_1_id, taxonomies).await;
}

async fn create_dirty_article_draft(client: &E2eClient, editor_jwt: &str, art_1_id: &str) {
    let edit_path = format!("/api/v1/content/article/{art_1_id}");
    let edit_body = parse_json_with_status(
        client
            .put_json(
                &edit_path,
                editor_jwt,
                &json!({
                    "fields": {
                        "title": "Dirty Working Draft V3",
                        "slug": "rust-testcontainers-arch",
                        "summary": {
                            "en": "Updated comprehensive guide to E2E testing with Testcontainers."
                        },
                        "priority": 1
                    }
                }),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(edit_body["data"]["publicationState"]["status"], "modified");
}

async fn assert_snapshot_restore_conflict(client: &E2eClient, editor_jwt: &str, art_1_id: &str) {
    let restore_path = format!("/api/v1/content/article/{art_1_id}/snapshots/1/restore");
    let conflict_resp = client.post_json(&restore_path, editor_jwt, &json!({})).await;
    assert_problem_details(conflict_resp, StatusCode::CONFLICT, "draft_has_pending_edits").await;
}

async fn execute_snapshot_restore_with_discard(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
    taxonomies: &SeededTaxonomies,
) {
    let restore_path = format!("/api/v1/content/article/{art_1_id}/snapshots/1/restore");
    let restored_body = parse_json_with_status(
        client
            .post_json(
                &restore_path,
                editor_jwt,
                &json!({ "discardPendingEdits": true }),
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &restored_body,
        expected: json!({
            "data": {
                "publicationState": { "status": "modified" },
                "fields": {
                    "title": "Rust Testcontainers Architecture",
                    "price": "19.99"
                },
                "relations": {
                    "category": [&taxonomies.cat_tech_id],
                    "tags": [&taxonomies.tag_rust_id]
                }
            }
        })
    );
}

async fn verify_article_discard_unpublish_and_delete(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
    art_2_id: &str,
) {
    discard_article_draft(client, editor_jwt, art_1_id).await;
    unpublish_article_and_verify_views(client, editor_jwt, art_1_id).await;
    verify_article_deletion(client, editor_jwt, art_2_id).await;
}

async fn discard_article_draft(client: &E2eClient, editor_jwt: &str, art_1_id: &str) {
    let discard_path = format!("/api/v1/content/article/{art_1_id}/discard-draft");
    let discard_body = parse_json_with_status(
        client.post_empty(&discard_path, editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &discard_body,
        expected: json!({
            "data": {
                "publicationState": {
                    "status": "published",
                    "revision": 2
                },
                "fields": {
                    "title": "Rust Testcontainers Architecture V2",
                    "price": "29.99"
                }
            }
        })
    );
}

async fn unpublish_article_and_verify_views(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
) {
    unpublish_article(client, editor_jwt, art_1_id).await;
    verify_unpublished_article_views(client, editor_jwt, art_1_id).await;
}

async fn unpublish_article(client: &E2eClient, editor_jwt: &str, art_1_id: &str) {
    let unpub_path = format!("/api/v1/content/article/{art_1_id}/unpublish");
    let unpub_body = parse_json_with_status(
        client.post_empty(&unpub_path, editor_jwt).await,
        StatusCode::OK,
    )
    .await;
    assert_json_include!(
        actual: &unpub_body,
        expected: json!({
            "data": {
                "publicationState": {
                    "status": "unpublished",
                    "lastPublishedRevision": 2
                }
            }
        })
    );
}

async fn verify_unpublished_article_views(
    client: &E2eClient,
    editor_jwt: &str,
    art_1_id: &str,
) {
    let pub_get = client
        .get(&format!("/api/v1/content/article/{art_1_id}"), editor_jwt)
        .await;
    assert_status(pub_get.status(), StatusCode::NOT_FOUND);

    let working_body = parse_json_with_status(
        client
            .get(
                &format!("/api/v1/content/article/{art_1_id}?view=working"),
                editor_jwt,
            )
            .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        working_body["data"]["publicationState"]["status"],
        "unpublished"
    );
}

async fn verify_article_deletion(client: &E2eClient, editor_jwt: &str, art_2_id: &str) {
    let art_2_path = format!("/api/v1/content/article/{art_2_id}");
    let del_resp = client.delete(&art_2_path, editor_jwt).await;
    assert_status(del_resp.status(), StatusCode::NO_CONTENT);

    let working_resp = client
        .get(&format!("{art_2_path}?view=working"), editor_jwt)
        .await;
    assert_status(working_resp.status(), StatusCode::NOT_FOUND);
}

