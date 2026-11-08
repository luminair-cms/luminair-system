# End-to-End Service Testing Plan (Testcontainers + Dex OIDC + PostgreSQL)

- **Target Crate**: `infrastructure` (production JWKS support + E2E integration test suite)
- **Selected Architecture**:
  - **Container Topology (Variant C1)**: Programmatic `testcontainers` (`0.28.0`) `GenericImage` orchestration running all 3 services (`postgres:18.6-alpine3.24`, `ghcr.io/dexidp/dex:v2.43.0`, and `luminair-cli:local` in `--mode serve --auto-migrate`) over an isolated Docker bridge network.
  - **OIDC / Dex Configuration (Option 3)**: Production-grade JWKS support (`--oidc-jwks-url` / `LUMINAIR_OIDC_JWKS_URL`) in `OidcTokenValidator` and `CliConfig` using `jsonwebtoken::jwk::JwkSet` (`11.1.0`) and `reqwest` (`0.13.5`), enabling full RS256 cryptographic signature, `kid`, `iss`, and `aud` validation across split-horizon Docker networks.
  - **JSON Assertion Strategy (Option A)**: Declarative structural subset matching via `assert-json-diff` (`2.0.2`) (`assert_json_include!`) paired with dedicated RFC 9457 `application/problem+json` assertion helpers.
- **Status**: In Progress
- **Last Updated**: 2026-11-08

---

## 1. Plan Overview & Step-by-Step Roadmap

```mermaid
flowchart TD
    S1["Phase 1: Workspace Dependencies & Production OIDC JWKS Support<br/>(Cargo.toml, OidcTokenValidator::from_jwk_set, --oidc-jwks-url in CliConfig & service.rs)"]
    S2["Phase 2: E2E Test Harness & Container Topology (Variant C1)<br/>(tests/e2e_support/: Docker network, Postgres + Dex + Luminair containers, HTTP client, RFC 9457 & JSON helpers)"]
    S3["Phase 3: E2E Scenarios — Auth, Onboarding & Single-Lifecycle Types<br/>(Health probes, Dex JWT acquisition, onboarding flow, category & tag CRUD + lifecycle guards)"]
    S4["Phase 4: E2E Scenarios — Singleton & Full Collection Lifecycle with Relations<br/>(site-settings singleton, article draft/publish/modify, set/connect/disconnect relations, snapshots, restore, discard, unpublish, delete)"]
    S5["Phase 5: Final Review, Documentation Sync & Verification Gates<br/>(Self-review checklist, README.md / docs/local-development.md update, clippy, unit tests, E2E test execution)"]

    S1 --> S2
    S2 --> S3
    S3 --> S4
    S4 --> S5
```

---

## 2. Detailed Implementation Phases

### Phase 1: Workspace Dependencies & Production OIDC JWKS Support (`Option 3`)

- [x] **Step 1.1: Register Latest Dependencies (`Cargo.toml` & `infrastructure/Cargo.toml`)**
  - Add `reqwest = { version = "0.13.5", default-features = false, features = ["json", "form", "query", ...] }` to `[workspace.dependencies]` and `infrastructure` `[dependencies]`.
  - Add `testcontainers = { version = "0.28.0" }` and `assert-json-diff = { version = "2.0.2" }` to `[workspace.dependencies]` and `infrastructure` `[dev-dependencies]`.
- [x] **Step 1.2: Add `JwkSet` Support to `OidcTokenValidator` (`infrastructure/src/auth/oidc.rs`)**
  - Extend `OidcConfig` with `pub jwks_json: Option<String>`.
  - Extend `ValidatorInner` with a `Jwks` variant holding pre-parsed `DecodingKey` entries (keyed by `Option<String>` `kid`, plus fallback key) and `Box<Validation>` for `Algorithm::RS256`.
  - Implement `OidcTokenValidator::from_jwk_set(jwk_set: &JwkSet, issuer: Option<String>, audience: Option<String>) -> Result<Self, AuthError>` and `OidcTokenValidator::from_jwks_json(jwks_json: &str, issuer: Option<String>, audience: Option<String>) -> Result<Self, AuthError>`.
  - Update `decode_claims` to inspect `jsonwebtoken::decode_header(token)` when using `ValidatorInner::Jwks`, resolve the matching key by `kid` (or single-key fallback), and verify the RS256 signature and claims (`exp`, `iss`, `aud`).
  - Add unit tests in `infrastructure/src/auth/oidc.rs` testing `from_jwks_json`, valid RS256 JWK verification, `kid` selection, and invalid signature rejection.
- [x] **Step 1.3: Add `--oidc-jwks-url` / `LUMINAIR_OIDC_JWKS_URL` to CLI & Service Startup (`infrastructure/src/cli/config.rs`, `service.rs`, `tests/cli_test.rs`)**
  - Add `pub oidc_jwks_url: Option<String>` to `CliConfig` and `CliConfigFileDto`.
  - Support `LUMINAIR_OIDC_JWKS_URL` in `apply_env_overrides` and `--oidc-jwks-url` in `apply_cli_arg_overrides`.
  - In `infrastructure/src/cli/service.rs`, make `build_auth_machinery` / `build_oidc_validator` async so that when `config.oidc_jwks_url` is configured, it fetches the JWKS JSON over HTTP (with retry/backoff for container startup ordering) and constructs `OidcTokenValidator::from_jwks_json`.
  - Update `docker-compose.yml` (`luminair-server` environment) and `README.md` with `LUMINAIR_OIDC_JWKS_URL`.
  - Add CLI parsing unit/integration tests for `--oidc-jwks-url` and `LUMINAIR_OIDC_JWKS_URL`.

#### Phase 1 Implementation Notes
- Registered `reqwest = 0.13.5` (features: `charset`, `http2`, `json`, `form`, `query`), `testcontainers = 0.28.0`, and `assert-json-diff = 2.0.2` in root workspace dependencies and `infrastructure`.
- Implemented `ValidatorInner::Jwks` in `infrastructure/src/auth/oidc.rs` supporting RS256 JWKS sets, key lookup by `kid`, fallback key when no kid is in token header, and rejection of malformed or empty JWKS.
- Added `jwks_json` field to `OidcConfig` and unit tests in `oidc.rs` covering RS256 verification and kid resolution.
- Added `--oidc-jwks-url` / `LUMINAIR_OIDC_JWKS_URL` CLI flags and env ingestion in `infrastructure/src/cli/config.rs`.
- Made service auth machinery asynchronous in `infrastructure/src/cli/service.rs` with `fetch_jwks_with_retry` (10 retries with 500ms backoff).
- Updated `docker-compose.yml` (`luminair-server` service) and `README.md` documentation. All unit tests and `cli_test` integration tests pass cleanly.

---

### Phase 2: E2E Test Harness & Container Topology (`Variant C1` + `Option A`)

- [x] **Step 2.1: Modular E2E Support Harness (`infrastructure/tests/e2e_support/`)**
  - `mod.rs`: Clean facade re-exporting `E2eEnvironment`, `E2eClient`, and assertion helpers.
  - `containers.rs`:
    - Ensures the `luminair-cli:local` Docker image is available (or builds it if missing / requested via `ensure_luminair_image_built`, respecting `LUMINAIR_SKIP_DOCKER_BUILD`).
    - Generates an isolated Docker network name (`luminair-e2e-<uuid_v7>`).
    - Starts **Container 1 (`postgres:18.6-alpine3.24`)** on the shared network with unique alias `postgres-<uuid>`, `POSTGRES_USER=luminair`, `POSTGRES_PASSWORD=luminair`, `POSTGRES_DB=luminair`, waiting for database readiness.
    - Starts **Container 2 (`ghcr.io/dexidp/dex:v2.43.0`)** on the shared network with unique alias `dex-<uuid>`, bind-mounting `config/dex.yaml` to `/etc/dex/config.yaml`, waiting for `/dex/healthz` readiness.
    - Starts **Container 3 (`luminair-cli:local`)** on the shared network, bind-mounting `config/` to `/app/config`, configuring `LUMINAIR_DATABASE_URL=postgres://luminair:luminair@{pg_alias}:5432/luminair`, `LUMINAIR_AUTO_MIGRATE=true`, `LUMINAIR_OIDC_ISSUER=http://localhost:5556/dex`, `LUMINAIR_OIDC_AUDIENCE=luminair-admin-spa`, `LUMINAIR_OIDC_JWKS_URL=http://{dex_alias}:5556/dex/keys`, and `LUMINAIR_SERVICE_TOKENS=local-dev-admin-token:ci-agent:admin`, waiting for `GET /ready` (`200 OK`) with container log dumping on failure.
    - Guarantees network cleanup via `DockerNetworkGuard` positioned as the trailing field of `E2eEnvironment`, ensuring container processes terminate and disconnect prior to `docker network rm -f`.
  - `client.rs`:
    - `E2eClient` wrapping `reqwest::Client`, `base_url` (`http://127.0.0.1:<luminair_host_port>`), and `dex_url` (`http://127.0.0.1:<dex_host_port>/dex`).
    - `fetch_dex_id_token(email, password) -> String`: Sends `POST /dex/token` with `Host: localhost:5556` (matching `dex.yaml`'s `issuer: http://localhost:5556/dex`) and `grant_type=password&scope=openid profile email&client_id=luminair-admin-spa`.
    - Ergonomic request helpers (`get_unauthenticated`, `get`, `post_json`, `post_empty`, `put_json`, `delete`) with Bearer token injection.
  - `assertions.rs`:
    - Re-exports `assert_json_diff::assert_json_include` and provides `parse_json_with_status`, `assert_problem_details(response, expected_status, expected_code)`, `assert_status`, and `extract_data_id(&Value) -> String`.

#### Phase 2 Implementation Notes
- Created modular test support structure in `infrastructure/tests/e2e_support/`:
  - `mod.rs`: Clean entry facade re-exporting `E2eEnvironment`, `E2eClient`, and assertion utilities.
  - `containers.rs`: Implemented `E2eEnvironment` orchestrating Postgres 18.6, Dex v2.43.0, and Luminair CLI containers over a dynamically generated Docker bridge network (`luminair-e2e-<uuid_v7>`). Container names use unique UUID suffixes to avoid collisions in concurrent executions. Struct drop order is leveraged via trailing `DockerNetworkGuard` to ensure all containers disconnect before network removal.
  - `client.rs`: Implemented `E2eClient` with `fetch_dex_id_token` supporting resource owner password flow with custom `Host` header matching Dex issuer URL, and ergonomic authenticated request helpers (`get`, `post_json`, `post_empty`, `put_json`, `delete`).
  - `assertions.rs`: Implemented RFC 9457 `application/problem+json` validation (`assert_problem_details`), status checking (`parse_json_with_status`, `assert_status`), and entity ID extraction (`extract_data_id`).
- Re-exported `OidcConfig` in `infrastructure/src/auth/mod.rs` to maintain clean library interface and eliminate binary-target dead code warnings.
- Added comprehensive unit tests in `assertions.rs` and `client.rs` verifying ID extraction, error formatting, and URL normalization.
- Validated that all unit tests pass, and verified full compilation and clippy compliance (`cargo check --tests -p infrastructure` and `cargo clippy --workspace --all-targets -- -D warnings`).

---

### Phase 3: E2E Scenarios — Auth, Onboarding & Single-Lifecycle Document Types

- [x] **Step 3.1: Health Probes, OIDC JWT Verification & Onboarding Flow (`infrastructure/tests/e2e_service_test.rs`)**
  - Verify `GET /healthz` and `GET /ready` return `200 OK` with `{"status": "healthy"}`.
  - Obtain real RS256 `admin_jwt` (`admin@luminair.dev`) and `editor_jwt` (`editor@luminair.dev`) from the Dex container.
  - Verify forged/invalid JWT is rejected with `401 Unauthorized` (confirming live RS256 JWKS signature verification).
  - Verify pre-onboarding `GET /api/v1/system/config` with `admin_jwt` returns `403 Forbidden` (`code: "access_not_requested"`).
  - Execute `POST /api/v1/auth/profile/sync` and `POST /api/v1/auth/access-requests` for both `admin` and `editor`.
  - Verify pending user gets `403 Forbidden` (`code: "access_pending"`).
  - Approve `admin`'s access request using the M2M service token (`local-dev-admin-token`) granting `ROLE_ADMIN_ID`.
  - Approve `editor`'s access request using `admin_jwt` granting `ROLE_EDITOR_ID`.
  - Verify `GET /api/v1/system/config`, `GET /api/v1/system/locales`, `GET /api/v1/schema/document-types` (4 schemas), and `GET /api/v1/auth/users` succeed.
- [x] **Step 3.2: Single-Lifecycle Collections (`category` & `tag`, `draftAndPublish: false`)**
  - Create 2 `category` instances (`"Technology"`, `"Science"`) and 3 `tag` instances (`"rust"`, `"docker"`, `"obsolete"`).
  - Verify immediate publication (`publicationState.status == "published"`, `revision == 1`) and visibility in default `GET /api/v1/content/category` and `GET /api/v1/content/tag`.
  - Verify `POST /api/v1/content/category/{id}/publish` and `/unpublish` return `409 Conflict` (`code: "draft_and_publish_disabled"`).
  - Update `"obsolete"` tag via `PUT` (`"name": "deprecated"`), verify `audit.version == 2`, then `DELETE` it (`204 No Content`) and verify `404 Not Found`.

#### Phase 3 Implementation Notes
- Created `infrastructure/tests/e2e_service_test.rs` following strict SLAP/SRP metrics (<40 lines per function, max nesting depth 1, newspaper item ordering).
- Imported `ROLE_ADMIN_ID` and `ROLE_EDITOR_ID` directly from `infrastructure::persistence::migration` (keeping `infrastructure::lib.rs` clean without redundant root re-exports).
- Implemented `verify_health_and_onboarding` covering health and readiness probes (`/healthz`, `/ready`), live RS256 signature verification and forged token rejection, onboarding states (`access_not_requested`, `access_pending`), bootstrap approval via M2M service token, delegated role assignment via human administrator, and post-onboarding system inspectability (`config`, `locales`, `schema`, user directory with query filter).
- Implemented `verify_single_lifecycle_collections` covering category and tag document creation, verified immediate publication (`publicationState.status == "published"`, `revision == 1`), lifecycle guard assertions returning 409 Conflict (`draft_and_publish_disabled`) for `/publish` and `/unpublish`, tag update with OCC/version increment (`audit.version == 2`), deletion returning 204 No Content, and confirmed 404 Not Found on subsequent fetch.
- All code passes `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.

---

### Phase 4: E2E Scenarios — Singleton & Full Collection Lifecycle with Relations

- [x] **Step 4.1: Singleton Lifecycle (`site-settings`, `kind: "singleton"`, `draftAndPublish: true`)**
  - Create `site-settings` instance (`"site-name": "Luminair Cloud"`, `"maintenance-mode": false`) $\rightarrow$ `201 Created` (`status: "draft"`).
  - Verify second `POST /api/v1/content/site-settings` fails with `409 Conflict` (`code: "singleton_already_exists"`).
  - Publish revision 1, update via `PUT` (`"maintenance-mode": true` $\rightarrow$ `status: "modified"`), verify `?view=published` vs `?view=working` isolation, and publish revision 2.
- [x] **Step 4.2: Full Collection Lifecycle, Relations & Snapshots (`article`, `draftAndPublish: true`)**
  - Create 2 `article` instances (`art_1`, `art_2`) exercising all field types (`text`, `uid`, `localizedText` in `en`/`uk`, `int32`, `decimal`, `json`) and initial relations (`category` and `tags`).
  - Verify `?view=published` (`total: 0`) vs `?view=working` (`total: 2`) and `/api/v1/content/article/admin/headers` (`total: 2`, `status: "draft"`).
  - Publish `art_1` (`revision: 1`), verify duplicate `/publish` returns `409 Conflict` (`code: "already_published"`), and verify `?populate=category,tags` returns populated target DTOs.
  - Verify OCC conflict (`expectedVersion: 999` $\rightarrow$ `409 version_mismatch`).
  - Update `art_1` using relational actions (`"category": {"set": [cat_sci]}`, `"tags": {"connect": [tag_docker]}}`) $\rightarrow$ `status: "modified"`, then test `"tags": {"disconnect": [tag_rust]}`.
  - Publish `art_1` (`revision: 2`), inspect `/snapshots` and `/snapshots/1`, make a dirty edit, verify `/snapshots/1/restore` without flag returns `409 draft_has_pending_edits`, and verify `/snapshots/1/restore` with `{"discardPendingEdits": true}` restores revision 1's fields and relations.
  - Execute `/discard-draft` (reverts to clean `Published` revision 2), `/unpublish` (`status: "unpublished"`), and `DELETE` `art_2` (`204 No Content` $\rightarrow$ `404 Not Found`).

#### Phase 4 Implementation Notes
- Implemented `verify_singleton_lifecycle` in `infrastructure/tests/e2e_service_test.rs`:
  - Verified creation of singleton draft (`site-settings`, `status: "draft"`).
  - Asserted `singleton_already_exists` guard on secondary creation attempts returning `409 Conflict`.
  - Tested revision 1 publish, draft modification transitions to `status: "modified"` (`liveRevision: 1`, `draftVersion: 3`), verified view isolation between `?view=published` and `?view=working`, and published revision 2.
- Implemented `verify_article_lifecycle_and_relations` in `infrastructure/tests/e2e_service_test.rs`:
  - Created 2 `article` instances exercising all schema types (`text`, `uid`, localized dictionary in `en`/`uk`, integer, decimal formatted as string, and JSON) alongside relation attributes (`category` hasOne, `tags` hasMany).
  - Verified draft view filtering (`view=published` returns total 0 while `view=working` and `/admin/headers` return total 2 drafts).
  - Verified initial publish, duplicate publish prevention (`already_published`), and populated relation resolution (`?populate=category,tags`).
  - Tested optimistic concurrency control guard with stale `expectedVersion` returning `409 version_mismatch`.
  - Tested relational mutations with `{ "set": [...] }`, `{ "connect": [...] }`, and `{ "disconnect": [...] }`.
  - Verified historical snapshot generation upon second publish, snapshot inspection at `/snapshots` and `/snapshots/1`, pending edit detection (`409 draft_has_pending_edits`) during restore attempts, and forced restore with `{ "discardPendingEdits": true }`.
  - Verified draft discard (`POST /discard-draft`), unpublishing (`POST /unpublish`), and document deletion (`DELETE` returning `204 No Content` and verifying subsequent `404 Not Found`).
- Strictly adhered to Clean Code, SRP, and SLAP guidelines (all functions $\le 38$ lines, maximum nesting depth 1-2, top-down newspaper ordering).
- All code passes `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.

---

### Phase 5: Final Review, Documentation Sync & Verification Gates

- [x] **Step 5.1: Code Review & Clean Code Audit (`.ai/skills/common/review.md`)**
  - Verify SLAP/SRP metrics (functions $\le 40$ lines, max nesting depth 2, newspaper ordering, zero `.unwrap()` in production code).
- [x] **Step 5.2: Run Automated Quality Gates**
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - Execute the full container-backed E2E test suite (`cargo test -p infrastructure --test e2e_service_test`).

#### Phase 5 Implementation Notes
- Executed a comprehensive review against `.ai/rules/common.md`, `.ai/rules/backend-rust.md`, `.ai/skills/backend/clean-code.md`, `.ai/skills/backend/rust-patterns.md`, `.ai/skills/backend/testing.md`, and `.ai/skills/common/review.md`, with particular focus on **visibility** (`pub` vs `pub(crate)` vs private) and **top-down newspaper ordering**:
  1. **Visibility & Facade Cleanup**:
     - `infrastructure/src/lib.rs`: Removed redundant root re-exports of `ROLE_*_ID` so `lib.rs` remains a clean module manifest; `e2e_service_test.rs` imports `ROLE_ADMIN_ID` and `ROLE_EDITOR_ID` from `infrastructure::persistence::migration`.
     - `infrastructure/src/auth/mod.rs` & `oidc.rs`: Restored `#[cfg(test)] struct OidcConfig` with private fields/methods in `oidc.rs` and removed `OidcConfig` from `pub use` in `auth/mod.rs`.
     - `infrastructure/src/cli/service.rs`: Scoped `execute_service_mode` to `pub(crate) async fn`.
     - `infrastructure/tests/e2e_support/`: Changed submodules in `e2e_support/mod.rs` from `pub mod` to private `mod` with explicit `pub use` facade exports; made `DockerNetworkGuard` and its `name` field private in `containers.rs`, removed unused `network_name` field/getter on `E2eEnvironment`, and made `base_url` and `dex_url` private in `E2eClient`.
     - `infrastructure/tests/e2e_service_test.rs`: Changed test-internal context structs (`OnboardedTokens`, `SeededTaxonomies`) and orchestrator functions from `pub` to private.
  2. **Top-Down Newspaper Ordering**:
     - `infrastructure/src/auth/oidc.rs`: Placed high-level caller `from_jwks_json` above `from_jwk_set` in `impl OidcTokenValidator`, and ordered `build_validation` before `decode_jwks_claims` in private helpers to match call order.
     - `infrastructure/tests/e2e_support/containers.rs`: Reordered private helpers in strict top-down call order matching `E2eEnvironment::start()` (`find_workspace_root` $\rightarrow$ `ensure_luminair_image_built` $\rightarrow$ `create_docker_network` $\rightarrow$ `start_postgres_container` $\rightarrow$ `start_dex_container` $\rightarrow$ `start_luminair_container` $\rightarrow$ `wait_for_http_ok` $\rightarrow$ `build_luminair_image` $\rightarrow$ `wait_for_luminair_ready` $\rightarrow$ `normalize_mount_path`).
     - `infrastructure/tests/e2e_service_test.rs`: Placed `is_docker_available` immediately below the top-level orchestrators and moved leaf payload builders (`first_article_payload`, `article_v2_set_connect_payload`, `article_v2_disconnect_payload`) below their callers (`create_first_article_draft`, `verify_article_relation_set_and_connect`, `verify_article_relation_disconnect`).
  3. **Documentation & Quality Gates**:
     - Fixed misplaced doc comments on `oidc_jwks_url` and `service_tokens` in `infrastructure/src/cli/config.rs`.
     - Verified `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass with zero errors.
