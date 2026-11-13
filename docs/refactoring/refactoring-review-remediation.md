# Refactoring Plan: Remediation of the Dual-Target Refactoring Review

- **Target Crates**: `infrastructure/persistence`, `infrastructure/http`, `infrastructure/auth`, `infrastructure/auth-cognito-edge`, `infrastructure/schema-loader`, `bootstrap/aws-lambda`, `bootstrap/luminair-server`, `bootstrap/luminair-cli`, legacy `infrastructure` (to be removed)
- **Source of Findings**: Review of commit `a9b9115` against [`refactoring-dual-target-lambda-and-microservice.md`](./refactoring-dual-target-lambda-and-microservice.md), [`ADR-007`](../adr/ADR-007-dual-target-deployment-lambda-and-microservice.md), and `.ai/` rules & skills
- **Status**: Proposed *(Plan only — not implemented)*
- **Date**: 2026-11-13

---

## 1. Finding Verification

Every finding from the review of commit `a9b9115` was re-verified against the source tree and library APIs (`sqlx-core` `0.9.0`, `lambda_http` `0.14.0`, `aws_lambda_events` `0.16.1`). Three additional findings (`N-01`, `N-02`, `N-03`) were discovered alongside `F-01`–`F-10`:

| ID | Finding | Verified Evidence | Severity |
|---|---|---|---|
| **F-01** | Aurora DSQL IAM token is generated once; recycled connections fail after 15 min | `build_aurora_dsql_pool` calls `generate_auth_token` once and passes static password to `PgConnectOptions`; `sqlx-core` `0.9.0` `PoolOptions` only exposes `after_connect` / `before_acquire` (existing connections), while replacing new-connection credentials requires `Pool::set_connect_options` | Critical |
| **F-02** | Legacy `infrastructure` crate, duplicate migrations, tests, `src/cli/`, and orphaned files remain | Root `Cargo.toml:6` still lists `"infrastructure"`; `infrastructure/Cargo.toml` compiles legacy binary; `infrastructure/src/` still holds `82` files; `infrastructure/migrations/` duplicates `infrastructure/persistence/migrations/` (`diff -rq` identical); all integration/E2E tests stay in `infrastructure/tests/` | High |
| **F-03** | Plan and `ADR-007` claim `domain` remained "100% untouched", but 2 additive edits exist | `domain/src/schema/attributes.rs` (`pub use rust_decimal::Decimal;`) and `domain/src/system/config.rs` (`SystemConfigId::generate()`) were modified; both preserve domain purity | Low |
| **F-04** | Missing edge-auth HTTP router & `aws-lambda` integration tests | `CognitoEdgeAuthResolver` only has mock-repo unit tests; `bootstrap/aws-lambda` has 0 tests; no Axum router test exercises edge auth through `Caller` / `OnboardingUser` | Medium |
| **F-05** | `.expect()` calls in production code (`bootstrap/aws-lambda/src/main.rs`) | Lines `21`, `24`, `29` call `.expect(...)` inside `main() -> Result<(), Error>`; violates `.ai/rules/backend-rust.md` §4 | High *(rule violation)* |
| **F-06** | Production functions > 40 lines / nesting > 2 (SLAP/SRP) | **28** production functions > 40 lines across `bootstrap/*` & `infrastructure/*`: **6** new/refactored (`luminair-server`: `main` 95, `fetch_jwks_with_retry` 42, `build_service_token_registry` 61; `luminair-http`: `extract_credentials` 46 + nesting 3; `luminair-persistence`: `generate_auth_token` 52, `generate_dsql_sigv4_token` 81) + **22** moved pre-existing (`luminair-cli` 1, `luminair-schema-loader` 4, `luminair-http` 3, `luminair-persistence` 14) | Medium *(rule violation)* |
| **F-07** | `ADR-007` governance process gaps | `ADR-007` is absent from `AGENTS.md` ADR index and `.ai/context/decisions.md` (`.ai/skills/common/adr-process.md` Phase 4); `Status: Accepted` (`2026-11-12`) lacks verifiable human sign-off record (`.ai/rules/common.md` §1) | Medium *(governance)* |
| **F-08** | SAM `ParameterMappings` YAML inconsistent (`Value` vs `Mapping`) and invalid SAM property | Refactoring plan uses `Value: ...`, whereas `ADR-007` and `docs/deploy-with-aws.md` use `Mapping: ...`; AWS SAM `HttpApi` event has no `ParameterMappings` property in that syntax | Medium |
| **F-09** | `deploy-with-aws.md` Dockerfile snippet mismatches `Dockerfile`; Compose `wget` healthcheck fails on distroless | `Dockerfile` uses `rust:1.99.1-bookworm` + `gcr.io/distroless/cc-debian12:nonroot`, vs `rust:1.82-bookworm` + `debian:bookworm-slim` in docs; `docker-compose.yml:155` runs `wget` against distroless `luminair-server` (no `wget` binary in image) | Medium |
| **F-10** | Unconditional `x-cognito-*` header inspection in all targets (`luminair-server` & `aws-lambda`) | `extract_credentials` checks `x-cognito-sub` before `Authorization: Bearer`; `SqlxAuthResolver` rejects `EdgeCognito` (preventing privilege escalation), but client-supplied `x-cognito-sub` + valid Bearer token fails with `401` | Medium *(security hardening)* |
| **N-01** | **Design flaw**: API Gateway HTTP API `ParameterMappings` do not apply to `AWS_PROXY` Lambda integrations | With HTTP API Payload Format 2.0 + JWT Authorizer, verified claims arrive in `requestContext.authorizer.jwt.claims` (`lambda_http::request::RequestContext::authorizer().jwt.claims`), not injected `x-cognito-*` headers | Critical |
| **N-02** | `aws-lambda` hard-codes pool size `1` and docs list unused `LUMINAIR_SAFETY_POLICY` | `bootstrap/aws-lambda/src/main.rs:27` ignores `LUMINAIR_MAX_DB_CONNECTIONS` (documented as clamped `1..2` in `docs/deploy-with-aws.md` §2.3 & §2.4); §2.3 & §2.4 also document `LUMINAIR_SAFETY_POLICY=additive` even though Lambda never runs migrations | Low |
| **N-03** | Stale docs & skills still reference monolithic `infrastructure`, `--mode serve`, and `luminair-cli:local` | Found in `AGENTS.md` (lines `23`, `79`), `docs/e2e-testing.md`, `docs/local-development.md`, `docs/plan-infrastructure.md`, `docs/plan-ui.md`, `docs/infrastructure/{overview,schema-loader,api-and-auth,persistence}.md`, and `.ai/skills/backend/testing.md` | Medium |

> [!IMPORTANT]
> **Human Governance Required (`F-07` & `N-01`)**:
> 1. Per `.ai/rules/common.md` §1, only humans can approve ADRs. Please confirm whether [`ADR-007`](../adr/ADR-007-dual-target-deployment-lambda-and-microservice.md) has explicit human approval for its existing `Status: Accepted` metadata.
> 2. Fixing `N-01` changes Decision 2 of `ADR-007` (API Gateway `ParameterMappings` → reading verified JWT authorizer claims from `requestContext.authorizer.jwt.claims`). Record that change inside `ADR-007` as a **`Proposed` Amendment** awaiting human acceptance before merging implementation.

---

## 2. Design Decisions for Remediation

### D1 — Aurora DSQL Dynamic Token Refresh (`F-01`)
- Add `infrastructure/persistence/src/refresh.rs` providing `DsqlTokenRefresher<P: DsqlTokenProvider>` + a `ManagedPool` wrapper tracking last token refresh timestamp and exposing the `PgPool` plus `ensure_fresh()`.
- On DSQL targets, `create_connection_pool` and `create_connection_pool_with_provider` build updated `PgConnectOptions` on each refresh cycle (every 5 minutes, well inside DSQL's 15-minute token TTL and 10-minute `max_lifetime`) and apply them via `PgPool::set_connect_options`, then spawn a background refresh task that exits cleanly when the pool closes (`tokio::spawn` lives strictly in `infrastructure/persistence`, never `domain`/`application`).
- Because Lambda execution environments freeze between invocations (pausing background timers), `aws-lambda` invokes `ManagedPool::ensure_fresh().await?` in a pre-request middleware when a token is stale (> 5 minutes old).

### D2 — Edge Identity via Trusted Request Extension (`N-01`, `F-10`, `F-08`)
- Introduce `luminair_http::auth::EdgeIdentity { pub user_id: String, pub email: Option<String>, pub name: Option<String> }` as an **Axum request extension**. HTTP clients cannot inject request extensions over the wire, eliminating header-spoofing risk by construction.
- Update `luminair_http::extractors::auth::extract_credentials` to check `parts.extensions.get::<EdgeIdentity>()` first (producing `AuthCredentials::EdgeCognito`), then fall back to `Authorization: Bearer`. Remove `x-cognito-*` header constants and header parsing.
- In `bootstrap/aws-lambda`, add Tower/Axum middleware that inspects `lambda_http::request::RequestContext::authorizer().jwt.claims` (from API Gateway HTTP API Payload Format 2.0) and inserts `EdgeIdentity` (extracting `sub`, plus optional `email` and `name`) when present. Unauthenticated requests without `sub` carry no `EdgeIdentity` and are rejected with `401`.
- Because `luminair-server` never attaches that middleware, `EdgeIdentity` can never appear in microservice requests (`F-10` resolved).
- Remove invalid `ParameterMappings` YAML blocks from `ADR-007` (via `Proposed` amendment), `docs/refactoring/refactoring-dual-target-lambda-and-microservice.md`, and `docs/deploy-with-aws.md` §2.4, retaining only the API Gateway HTTP API `CognitoAuthorizer`.

### D3 — Remove Legacy `infrastructure` Crate (`F-02`)
Relocate integration and E2E test suites directly to the owning workspace crates (preserving behavior and assertions, updating crate imports and `[dev-dependencies]` only):

| Legacy Location | Target Crate Location | Notes |
|---|---|---|
| `infrastructure/tests/api_test.rs` | `infrastructure/auth/tests/api_test.rs` | Uses `luminair-http` + `SqlxAuthResolver` (`luminair-auth`) |
| `infrastructure/tests/persistence_test.rs`, `migrations_test.rs` | `infrastructure/persistence/tests/` | Uses `luminair-persistence` |
| `infrastructure/tests/schema_loader_test.rs` | `infrastructure/schema-loader/tests/` | Uses `luminair-schema-loader` |
| `infrastructure/tests/cli_test.rs` | `bootstrap/luminair-cli/tests/cli_test.rs` | Tests `luminair-cli` binary/crate |
| `infrastructure/tests/e2e_service_test.rs`, `e2e_support/` | `bootstrap/luminair-server/tests/` | Tests `luminair:local` (`luminair-server` + `luminair-cli`) |
| `infrastructure/src/container.rs` tests | `infrastructure/http/src/container.rs` | Unit tests using fake `AuthContextResolver` |

Then remove `"infrastructure"` from root `Cargo.toml` and delete `infrastructure/Cargo.toml`, `infrastructure/src/` (all 82 legacy/orphaned files), `infrastructure/migrations/` (identical duplicate of `infrastructure/persistence/migrations/`), and `infrastructure/tests/`. Keep `Dockerfile` (`COPY infrastructure ./infrastructure` and `-p luminair-cli -p luminair-server` already match the decomposed workspace).

### D4 — Distroless Healthcheck (`F-09`)
- Add a dependency-free `--healthcheck` flag (or subcommand/flag parsed before daemon startup) to `bootstrap/luminair-server` that performs an internal TCP/HTTP `GET /healthz` probe against `127.0.0.1:$LUMINAIR_HTTP_PORT` and exits `0`/`1`.
- Update `docker-compose.yml` line `155` from `["CMD", "wget", ...]` to `["CMD", "luminair-server", "--healthcheck"]` so healthchecks succeed on `gcr.io/distroless/cc-debian12:nonroot`.

### D5 — Error Propagation, SLAP Decomposition & Lambda Config (`F-05`, `F-06`, `N-02`)
- **Typed Lambda bootstrap (`F-05`, `N-02`)**: Extract a pure `LambdaConfig::from_env()` helper parsing `LUMINAIR_SCHEMA_DIR`, `LUMINAIR_DATABASE_URL`, and `LUMINAIR_MAX_DB_CONNECTIONS` (default `1`, leveraging `resolve_pool_max_connections` to clamp `1..=2` on Lambda). Replace all three `.expect(...)` calls in `bootstrap/aws-lambda/src/main.rs` with typed error propagation via `?` and `thiserror`.
- **Server bootstrap & auth separation (`F-06`)**: Decompose `bootstrap/luminair-server/src/main.rs` into focused <= 30-line functions (`ServerConfig::from_env`, `connect_database`, `run_auto_migrations`, `build_auth_resolver`, `serve`). Move `fetch_jwks_with_retry` into `luminair-auth::oidc` and `parse_service_tokens` into `luminair-auth::service_token` so `bootstrap/luminair-server` drops direct HTTP/JSON parsing responsibilities and its direct `reqwest` dependency.
- **SLAP parity across refactored crates (`F-06`)**: Split remaining > 40-line functions in new and moved files into single-responsibility helpers <= 30 lines and nesting depth <= 2 without altering behavior (verified by keeping all existing tests unmodified).

---

## 3. Step-by-Step Implementation Plan

### Phase 1 — Critical Correctness & Security
- [ ] **1.1 Aurora DSQL token refresh (`F-01`)**: Add `infrastructure/persistence/src/refresh.rs` (`DsqlTokenRefresher`, `ManagedPool`), wire `Pool::set_connect_options` periodic refresh + `ensure_fresh()` into `create_connection_pool` / `create_connection_pool_with_provider`, and add unit tests using a mock `DsqlTokenProvider` (refresh updates pool connect options, stale threshold triggers refresh, background loop exits cleanly on `pool.close()`).
- [ ] **1.2 Trusted `EdgeIdentity` extension (`N-01`, `F-10`)**: Define `luminair_http::auth::EdgeIdentity`, update `extract_credentials` in `infrastructure/http/src/extractors/auth.rs` to inspect `EdgeIdentity` then `Authorization: Bearer` (keeping function <= 30 lines, nesting <= 2), remove `x-cognito-*` header constants/parsing, and add unit tests confirming spoofed `x-cognito-*` headers are ignored even alongside Bearer tokens.
- [ ] **1.3 `aws-lambda` middleware, typed config & tests (`N-01`, `F-05`, `F-04`, `N-02`)**: Split `bootstrap/aws-lambda/src/` into `config.rs`, `edge.rs`, `error.rs`, `app.rs`, `lib.rs`, and thin `main.rs`; extract `requestContext.authorizer.jwt.claims` into `EdgeIdentity`; invoke `ensure_fresh()` middleware before handling; eliminate all `.expect()` calls; honor `LUMINAIR_MAX_DB_CONNECTIONS`; write unit tests using API Gateway v2 `lambda_http::request::from_str` fixtures.

### Phase 2 — Legacy Cleanup & Edge-Auth HTTP Tests
- [ ] **2.1 Relocate integration & E2E tests (`F-02`)**: Move `infrastructure/tests/*` files to owning crates per Decision D3, fix crate imports/`Cargo.toml` dev-dependencies, and keep test behavior/assertions unchanged (`ADR-001` compliant).
- [ ] **2.2 Remove legacy `infrastructure` crate (`F-02`)**: Remove `"infrastructure"` from root `Cargo.toml`, regenerate `Cargo.lock` cleanly (`cargo check --workspace`), delete `infrastructure/Cargo.toml`, `infrastructure/src/`, `infrastructure/migrations/`, and `infrastructure/tests/`, and verify `Dockerfile` builds cleanly.
- [ ] **2.3 Edge-auth HTTP integration tests (`F-04`)**: Add `axum-test` integration tests (in `infrastructure/auth-cognito-edge/tests/` or `infrastructure/http/tests/`) exercising `AppContainer<CognitoEdgeAuthResolver>` + `build_router` with `EdgeIdentity` extensions across `Caller` / `OnboardingUser` and rejecting missing/spoofed headers with `401`/`403`.

### Phase 3 — Server Bootstrap & Distroless Healthcheck
- [ ] **3.1 Decompose `luminair-server` & add `--healthcheck` (`F-06`, `F-09`)**: Refactor `bootstrap/luminair-server/src/main.rs` into small SLAP-compliant modules/functions (`<= 30` lines), relocate `fetch_jwks_with_retry` & service-token parsing into `luminair-auth`, implement `luminair-server --healthcheck`, and update `docker-compose.yml:155` to run `["CMD", "luminair-server", "--healthcheck"]`.

### Phase 4 — Governance & Documentation Alignment
- [ ] **4.1 Update ADR index & decisions log (`F-07`)**: Add `ADR-007` to the ADR index table in `AGENTS.md`, record the dual-target decision summary in `.ai/context/decisions.md`, and request human confirmation of `ADR-007` `Status: Accepted`.
- [ ] **4.2 Fix AWS docs, SAM example & `ADR-007` amendment (`F-03`, `F-07`, `F-08`, `F-09`, `N-01`, `N-02`)**:
  - Add a `Proposed` Amendment section to `docs/adr/ADR-007-dual-target-deployment-lambda-and-microservice.md` revising Decision 2 (`ParameterMappings` → `requestContext.authorizer.jwt.claims`) and noting the two additive `domain` helpers (`F-03`).
  - Update `docs/refactoring/refactoring-dual-target-lambda-and-microservice.md` accordingly (`domain` additive changes, `N-01` claim extraction).
  - Update `docs/deploy-with-aws.md`: remove `ParameterMappings` YAML and explain API Gateway v2 JWT Authorizer payload claims (`§2.1`, `§2.4`), remove unused `LUMINAIR_SAFETY_POLICY` from Lambda env docs (`§2.3`, `§2.4`), and align the Dockerfile snippet in `§3.2` with `Dockerfile` (`rust:1.99.1-bookworm` + `gcr.io/distroless/cc-debian12:nonroot`).
- [ ] **4.3 Refresh stale repository docs & skills (`N-03`)**: Update references from monolithic `infrastructure` / `--mode serve` / `luminair-cli:local` to current crates, binaries, test paths, and `luminair:local` across:
  - `AGENTS.md` (line `23` `luminair:local`, line `79` `infrastructure/*` / `bootstrap/*`)
  - `docs/e2e-testing.md` (`bootstrap/luminair-server/tests/e2e_service_test.rs`, `-p luminair-server`, `luminair:local`, default `luminair-server` command)
  - `docs/local-development.md` (separate `luminair-cli` and `luminair-server` invocations, updated migration paths under `infrastructure/persistence/migrations/`)
  - `docs/plan-infrastructure.md` (annotate Steps 1–7 pre-split history vs Step 8 structure, remove `ParameterMappings` reference)
  - `docs/plan-ui.md` (fix source links to `infrastructure/http/src/*` and `infrastructure/persistence/migrations/*`)
  - `docs/infrastructure/{overview,schema-loader,api-and-auth,persistence}.md` (update target crate names, `infrastructure/persistence/migrations/`, and `luminair-server` runtime)
  - `.ai/skills/backend/testing.md` (update test file locations, `luminair:local`, and `luminair-server`)
  - `docs/api.md` (add concise note under auth overview explaining Lambda Edge Cognito API Gateway authorizer claim resolution alongside `Authorization: Bearer <JWT>`)
  - *(Leave `docs/adr/ADR-006-user-profile-and-directory-architecture.md` untouched as a historical accepted ADR.)*

### Phase 5 — Newly Written Long Functions (`connection.rs`)
- [ ] **5.1 Decompose `connection.rs` token builders (`F-06`)**: Split `generate_auth_token` (52 lines) and `generate_dsql_sigv4_token` (81 lines) in `infrastructure/persistence/src/connection.rs` plus any helper touched in Phases 1–3 into `<= 30`-line SLAP helpers (nesting depth `<= 2`).

### Phase 6 — Pre-existing Moved Long Functions
- [ ] **6.1 Decompose 22 pre-existing > 40-line functions (`F-06`)**:
  - `bootstrap/luminair-cli/src/config.rs`: `apply_cli_arg_overrides` (1)
  - `infrastructure/schema-loader/src/loader.rs`: `parse_document_info`, `parse_attributes`, `parse_field_type`, `parse_constraints` (4)
  - `infrastructure/http/src/`: `routes.rs::content_routes`, `codec/request.rs::decode_localized_text`, `codec/request.rs::decode_relation_object` (3)
  - `infrastructure/persistence/src/`: `repositories/{document_instance,relations}.rs`, `repositories/mappers/document_row.rs`, `query/wide_table.rs`, `migration/dynamic/{builder,introspector,planner}.rs` (14)

### Phase 7 — Verification Gates (Post-Implementation)
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings)
- [ ] `cargo test --workspace` (all unit/integration tests pass; run E2E suite where Docker is available via `cargo test -p luminair-server --test e2e_service_test`)
- [ ] `cargo tree -p luminair-cli -e normal | grep -E "axum|lambda|reqwest|jsonwebtoken"` → empty
- [ ] `cargo tree -p aws-lambda -e normal | grep -E "jsonwebtoken|reqwest"` → empty
- [ ] `cargo tree -p luminair-server -e normal | grep -E "lambda"` → empty; verify `bootstrap/luminair-server/Cargo.toml` drops direct `reqwest` once JWKS fetching moves to `luminair-auth`
- [ ] Automated static scans: `0` `.unwrap()` / `.expect()` outside tests; `0` production functions `> 40` lines or nesting `> 2` in refactored crates
- [ ] Documentation grep confirms no stale references to deleted `infrastructure` crate paths, `--mode serve`, or `luminair-cli:local`
- [ ] Execute [`.ai/skills/common/review.md`](../../.ai/skills/common/review.md) checklist in full

---

## 4. Risks & Environment Limits

- **Docker unavailable in sandbox**: Testcontainers E2E (`test_e2e_full_service_lifecycle`) compiles but skips execution when `/var/run/docker.sock` is inaccessible; container build and full E2E execution must be verified on a Docker-enabled host after implementation.
- **No live AWS DSQL / API Gateway in local CI**: DSQL refresh loops and API Gateway Payload Format 2.0 JWT authorizer extraction must be verified with injected mock `DsqlTokenProvider`s and realistic `lambda_http` event fixtures locally, then smoke-tested in a staging AWS account.
- **`cargo-audit` not installed**: Dependency advisory audit cannot run automatically in this environment and remains an external verification step.
