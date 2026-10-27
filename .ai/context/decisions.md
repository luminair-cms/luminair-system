# Running Log of Key Decisions

Complement to Architecture Decision Records (ADRs). Records tactical and operational decisions across milestones.

---

### 2026-10-06 — ADR-001 Accepted & Spec-Driven Testing Approach

- **Context**: Milestone 1 complete, `application` crate implemented with use cases and in-memory fakes. Decision trigger for ADR-001 reached.
- **Decisions**:
  1. **Accepted ADR-001**: Adopt hybrid testing model:
     - **Option 2 (Case Tables)** for domain state machines (`DocumentInstance` lifecycle) and decision tables (`AuthorizationService::can`).
     - **Option 3 (Markdown Given/When/Then Specs)** for application use case services (`DocumentsService`, `AccessRequestsService`).
     - **Option 4 (`proptest`)** for domain value object invariants (`AttributeId`, `DocumentTypeId`, `LocaleId`, `RegexPattern`).
  2. **Test Location**: Spec-backed tests live in `<crate>/tests/` (integration test directory) compiling against public APIs only to ensure black-box verification and prevent AI generation from reaching private internals. Legacy unit tests remain inline.
  3. **Table Representation**: Case tables implemented directly as typed Rust structures/enums in `tests/` instead of YAML/TOML, eliminating external DSL maintenance and dependencies while maintaining strict compile-time checks.
  4. **Fakes Hardening**: Repaired `FakeDocumentInstanceRepository` to honor `FieldFilter` on `find_by_type` and `count`, return `DomainError::Storage` instead of `Unauthorized` on poisoned locks, and maintain deterministic pagination sorting.
  5. **Traceability Gate**: Created `domain/tests/spec_traceability.rs` to mechanically ensure all `Approved` specification rules have corresponding tests and that tests reference valid rules.
  6. **Postponed Items**: `db_row_id` surrogate key cleanup and strict encapsulation of mutable aggregate fields are deferred to Milestone 2 (persistence implementation).

---

### 2026-10-09 — ADR-004 Revised: Asymmetric Hybrid Persistence Pattern

- **Context**: Milestone 2 preparation. Evaluation of AWS Aurora DSQL non-transactional DDL and draft invariants revealed that symmetric dual-wide-tables per document type (`articles` + `articles_published`) causes high migration friction, duplicate DDL, and rigid schema constraints on work-in-progress drafts.
- **Decisions**:
  1. **Asymmetric Pattern**: Published instances reside in a typed wide relational table (`[type]`); working drafts reside in a unified static JSONB table (`document_drafts`); revisions reside in a unified static append-only JSONB table (`document_snapshots`).
  2. **Distributed Query Defense**: Added `has_pending_draft BOOLEAN NOT NULL DEFAULT FALSE` to the wide published table to eliminate cross-shard distributed `FULL OUTER JOIN`s in Aurora DSQL for `list_admin_headers`.
  3. **Composite Identity**: Schemas declare `display_fields: Vec<AttributeId>`, materialized at write-time into `display_values JSONB` (`IndexMap<AttributeId, String>`) and a joined text fallback `display_title VARCHAR(255)`.
  4. **CQRS Port Separation**: Split repository query operations into `find_published` (wide table), `list_admin_headers` (lightweight projections), and `find_by_id(type_id, id, view: VersionView)`.
  5. **Concurrency & Drift Guards**: Added `expected_version` concurrency check and `schema_version` on drafts; infrastructure layer enforces DSQL OCC retry loops (`40001` serialization failure).
  6. **See**: [`ADR-004`](../docs/adr/ADR-004-draft-and-publish-table-pattern.md).

---

### 2026-10-20 — Actor Attribution (`ActorId`), Display Field Limits, & Read-Time Relational Enrichment

- **Context**: Finalizing the merged `domain` and `application` refactoring plan before step-by-step execution via Gemini Flash subagents.
- **Decisions**:
  1. **Strongly-Typed Mandatory `ActorId`**: Replace `Option<UserId>` across `AuditTrail`, `PublicationState`, `DocumentInstance` lifecycle methods (`new`, `touch`, `publish`, `unpublish`, `edit_draft`), and `CallerContext` with a mandatory `ActorId` enum (`ActorId::User(UserId) | ActorId::Service(ServiceId) | ActorId::System`). Serializes to compact URN strings (`"user:<id>"`, `"service:<id>"`, `"system"`) for database `VARCHAR(255) NOT NULL` storage.
  2. **`display_fields` Constraints**: Schema registration enforces **at most 3 display fields total** (`display_fields.len() <= 3`) and **at most 1 singular relational display field** (`HasOne` or `BelongsToOne`; multi-relations `HasMany` / `BelongsToMany` are rejected).
  3. **Read-Time 2-Phase Batch Enrichment for Related Display Values**: Write-time `extract_display_identity` extracts only scalar and localized scalar values into `display_values: IndexMap<AttributeId, String>`. The single allowed relational display field is resolved at read time inside `DocumentsService::list_admin_headers` via at most 1 batched lookup (`find_display_titles_by_ids`) per page (~25 rows), avoiding write-time event propagation complexity.
  4. **Domain Schema Purity**: `DocumentType`, `FieldDefinition`, and `RelationDefinition` remain free of Serde derives; JSON config loading and REST `GET` endpoints use dedicated DTOs in `infrastructure`.
  5. **See**: [`docs/refactoring-domain-and-application.md`](../docs/refactoring-domain-and-application.md).

---

### 2026-10-21 — ADR-004 Accepted & Domain/Application Review Hardening (Batches 1–5)

- **Context**: Completing implementation and spec approval for the Asymmetric Hybrid Persistence Pattern (ADR-004) across `domain` and `application`.
- **Decisions**:
  1. **Atomic Snapshot Transactions & Port Realignment**: `DocumentInstanceRepository::publish` and `DocumentInstanceRepository::unpublish` accept `snapshot: &DocumentSnapshot` directly (along with `display_values` and `display_title`) so that the wide published table, `document_drafts`, and `document_snapshots` are committed atomically within a single database transaction. The legacy monolithic `save` method was completely removed in favor of explicit lifecycle-specific persistence operations.
  2. **ISP Decoupling of Repositories in DocumentsService**: Decoupled `DocumentsServiceImpl<R, S = R>` using Interface Segregation Principle with constructors `new` (when `R` implements both instance and snapshot repositories) and `new_with_snapshots` (when independent repository implementations are provided).
  3. **Universal Actor Attribution in Auth Entities**: Extended `ActorId` attribution to `AccessRequest.reviewed_by: Option<ActorId>` and `UserRoleAssignment.granted_by: Option<ActorId>`, with approval/rejection taking `impl Into<ActorId>` and serialized to database URN strings.
  4. **Single-Lifecycle Immediate Publication**: For document types with `draft_and_publish: false`, `create` immediately publishes at `revision: 1` and persists to the wide table atomically with an initial snapshot, `update` immediately advances the published revision, and explicit `publish` or `unpublish` invocations return `ApplicationError::Conflict`.
  5. **Schema Graph Cross-Validation**: `SchemaRegistry::try_new` and `SchemaRegistry::validate` validate the full schema graph on registration, enforcing `display_fields` rules, field constraint applicability, and relation target existence across all declared types.
  6. **Identity Formatting & Reconstitution**: Added `format_display_title` for canonical composite title generation, `DocumentHeader::enrich_relational_display` for in-memory read-time Phase 2 relation enrichment, and `DocumentInstance::reconstitute` for pure repository entity reconstruction without triggering mutation events.

---

### 2026-10-21 — Archive-on-Replace Snapshots, Unpublish Draft Resolution & Checkpoint on Restore

- **Context**: Resolving snapshot redundancy and aligning single-document editorial workflows with Strapi v5 parity.
- **Decisions**:
  1. **Archive-on-Replace Snapshots (`SnapshotReason::Publish`)**: On initial publication (`revision = 1`), no snapshot is created because the content is actively live in `[type]`. When publishing revision `N` to replace a live revision `N - 1`, the superseded live version is archived to `document_snapshots` atomically via `publish(..., archived_snapshot: Option<&DocumentSnapshot>)`.
  2. **Strapi-Aligned Unpublish Resolution (`UnpublishDraftResolution`)**: Unpublishing takes the document completely offline (`[type]` row deleted, live version archived to `document_snapshots` with `SnapshotReason::Unpublish`). For `Modified` documents, callers choose via `UnpublishDraftResolution`: `KeepDraft` (default, preserves draft edits in `document_drafts`) or `ReplaceWithPublished` (replaces draft content with the live published content).
  3. **Automatic Checkpoint on Restore (`SnapshotReason::Checkpoint`)**: Restoring a historical snapshot (`restore_snapshot`) when the document is in `Modified` state automatically saves the uncommitted draft edits to `document_snapshots` as a checkpoint (`reason: SnapshotReason::Checkpoint`, `revision: draft_version`) before overwriting `document_drafts`.
  4. **Unified Symmetric `find_by_id(type_id, id, view: VersionView)`**: Removed asymmetric `find_draft_by_id` / `find_published_by_id` methods in favor of a single repository method accepting `VersionView::Published` (default for visitors and the read-only published tab) and `VersionView::Working` (for the editable draft tab).
  5. **See**: [`docs/refactoring-publish-unpublish-snapshots.md`](../docs/refactoring-publish-unpublish-snapshots.md).

---

### 2026-10-22 — Checkpoint Removal, Draft Unit Variant & Overwrite Confirmation on Restore

- **Context**: Resolving snapshot identity collisions, simplifying the `Draft` state representation, and protecting uncommitted draft edits without synthetic checkpoints.
- **Decisions**:
  1. **Removal of Checkpoints**: Removed `SnapshotReason::Checkpoint`. `document_snapshots` is an immutable, strictly unique archive of live revisions that have left the wide table (`reason IN ('publish', 'unpublish')`). The schema enforces `UNIQUE (document_type_id, document_id, revision)`, eliminating any collision between working draft versions and published revisions.
  2. **Overwrite Confirmation on Restore (`RestoreSnapshotCommand::with_discard_pending_edits`)**: When `restore_snapshot` is called on a document in `Modified` state (which has uncommitted working draft edits in `document_drafts`), it returns `ApplicationError::Conflict("restoring snapshot would overwrite pending working draft edits; confirm with discard_pending_edits: true")` unless explicitly confirmed via `with_discard_pending_edits()`.
  3. **`PublicationState::Draft` Unit Variant**: A draft document has never been published and carries no snapshots. Simplified `PublicationState::Draft { last_published_revision: Option<u32> }` to unit variant `PublicationState::Draft`. Calling `restore_snapshot` on `Draft` returns `DomainError::InvalidStateTransition` (`LC-13`).
  4. **Trash, Undelete & Retention Architecture (`ADR-005`)**: Proposed deletion rollback via `document_tombstones` table, preserving revision snapshots in `document_snapshots` until asynchronous vacuum retention purge, with an atomic hard erasure path for GDPR compliance.
  5. **Standardized Error Codes & Canonical RFC 9457 Problem Details Schema**: Replaced unstructured string conflicts with typed `ConflictCode` and structured metadata payload on `ApplicationError::Conflict`. Implemented RFC 9457 `ProblemDetails` (`type`, `title`, `status`, `code`, `detail`, `details`, `invalid_params`) and published `docs/api.md` with complete error code catalog, third-party developer guide, and Admin Studio UI bilingual localization mapping.
  6. **Hexagonal Boundary for HTTP Error Machinery**: Documented in `docs/architecture.md` and `application/src/errors.rs` that `ProblemDetails` and HTTP status mapping are transport concerns temporarily located in `application::errors` as a bridge; they will be moved to `infrastructure` (the HTTP driving adapter) when the Axum REST endpoints are implemented, leaving `application` completely transport-agnostic.

---

### 2026-10-23 — Infrastructure Crate Structure & Step-by-Step Implementation Plan

- **Context**: Transitioning from domain/application purity to infrastructure implementation. Defining the composition root, driving/driven adapters, and CLI entry point.
- **Decisions**:
  1. **Infrastructure Layout**: Organized `infrastructure` into `api` (Axum REST driving adapter), `auth` (OIDC JWT & Service tokens), `cli` (runner: Service, Migrate, DryRun), `persistence` (naming, migration, query, repositories), `schema_loader` (JSON file loader, DTOs, sanitization, validation), and `container.rs` (`AppContainer` composition root).
  2. **Step-by-Step Strategy**: Adopted a 7-phase incremental implementation plan starting with Phase 1 (`schema_loader` & JSON schema design) to provide a working, offline-testable `SystemContext` before connecting database and HTTP layers.
  3. **See**: [`docs/plan-infrastructure.md`](../docs/plan-infrastructure.md).

---

### 2026-10-23 — DocumentKind Singleton Alignment & Schema Loader DTO Decoupling

- **Context**: Decoupling domain types from infrastructure wire records and standardizing terminology for single-instance document types.
- **Decisions**:
  1. **Ubiquitous Language (`Singleton`)**: Renamed variant `DocumentKind::SingleType` to `DocumentKind::Singleton` in `domain::schema::DocumentKind` and `infrastructure::schema_loader::dto::DocumentKindDto`. Added `DocumentTypeBuilder::singleton` in test builders (retaining `single` as alias).
  2. **DTO Isolation & Deserialization Aliases**: Retained dedicated `DocumentKindDto` in `infrastructure::schema_loader::dto` to conform to Hexagonal/DDD Anti-Corruption Layer boundaries. Variant `Singleton` deserializes as `"singleton"`, with Serde aliases for `"singleType"` and `"singleton"` (`#[serde(alias = "singleton", alias = "singleType")]`).
  3. **Constraint Key Harmonization (`min*`)**: Standardized on canonical `min*` / `max*` prefixing (`minLength`, `maxLength`, `min`, `max`) and eliminated contradictory `minimalLength` / `maximalLength` keys. Supported aliases are `minInteger` / `maxInteger`, `minDecimal` / `maxDecimal`, `minimum` / `maximum`, and `regex` (for `pattern`).
  4. **Database Naming & Sanitization Relocation (`persistence::naming`)**: Moved `sanitize.rs` (`kebab_to_snake`, `RESERVED_SQL_KEYWORDS`, `is_reserved_sql_keyword`) from `schema_loader` to its architectural home in `infrastructure::persistence::naming`. `schema_loader::loader` now consumes it as an early boot-time validation dependency.
  5. **Integer Type Name Harmonization**: Unified integer field type descriptors across shorthand and structured formats into consistent fixed-width / descriptive pairs: `int16` / `short`, `int32` / `integer`, and `int64` / `long`. Removed arbitrary aliases (`i16`, `i32`, `i64`, `int`, `bigint`).
  6. **Code Organization & Item Ordering ("Newspaper Style")**: Adopted repository-wide Rust item ordering rules codified in `.ai/skills/rust-patterns.md` and enforced in `.ai/skills/review.md`:
     - Types first (constants, structs, enums) before functions and `impl` blocks.
     - Public functions/methods first, private helper functions after.
     - Dependency order ("newspaper style"): top-level entry points and orchestrators appear first, followed top-down by the functions they invoke down to leaf helpers.
     - Unit tests (`mod tests`) placed at the very bottom.
     - Reordered `infrastructure/src/schema_loader/loader.rs`, `dto.rs`, and `domain/src/schema/attributes.rs`, `types.rs` to conform.
  7. **Clean Code & Automatic Post-Coding Review Enforcement**:
     - Created dedicated skill [`.ai/skills/clean-code.md`](../.ai/skills/clean-code.md) codifying Single Responsibility Principle (SRP), Single Level of Abstraction Principle (SLAP), function size limits (10–30 lines), low nesting (max 2), Hexagonal Architecture, and Anti-Corruption Layer.
     - Established mandatory mechanics in `AGENTS.md`, `.ai/skills/review.md`, `.ai/agents/gemini/GEMINI.md`, and `.ai/agents/claude/CLAUDE.md` requiring automatic execution of the full review checklist immediately after every coding or refactoring task.
     - Refactored `load_schemas_from_dir` (from ~100 to 8 lines) and `parse_document_type` (from ~145 to 16 lines) into SLAP/SRP-compliant orchestrators with focused private worker helpers.





---

### 2026-10-23 — Phase 2: Database Persistence Foundation Complete

- **Context**: Implementing Phase 2 of the Infrastructure Implementation Plan (`docs/plan-infrastructure.md`). Provisioning SQL identifier mapping, static system migrations, dynamic additive schema synchronizer, Sea-Query builders, and OCC concurrency retry machinery for AWS Aurora DSQL and PostgreSQL portability.
- **Decisions**:
  1. **SQL Identifier Naming Engine (`persistence::naming::identifiers`)**:
     - Standardized lifecycle column constants (`id`, `revision`, `has_pending_draft`, `display_values`, `display_title`, etc.) and system table names.
     - Singletons resolve to `singular_name` in snake_case; collections resolve to `plural_name` in snake_case (`table_name_for_document_type`).
     - Junction tables resolve uniformly to `[source_table]_[rel_attr]_links` with `source_id UUID`, `target_id UUID`, `order_index INT`.
  2. **Static Migrations (`20261010000001` & `20261010000002`)**:
     - Embedded `document_drafts` (composite primary key `(document_type_id, id)`, JSONB `display_values`, `content`, `relations`, status checks) and `document_snapshots` (immutable revision history with `uq_document_snapshots_revision` constraint).
     - Both strictly start with `-- no-transaction` for AWS Aurora DSQL compatibility; no sequences or auto-increment columns; client UUIDv7 keys.
  3. **Dynamic Additive Schema Synchronizer (`persistence::migration::dynamic`)**:
     - Upgraded from simple string generation to a 5-stage AST-driven migration engine:
       1. **Typed Model (`model.rs`)**: `DatabaseSchema`, `TableDefinition`, `ColumnDefinition`, `IndexDefinition`, `ForeignKeyDefinition` with $O(1)$ lookups via `Borrow<str>`.
       2. **Desired Schema Projection (`builder.rs`)**: Projects `SchemaRegistry` into normalized `DatabaseSchema` AST adhering to ADR-004 wide entity and junction table conventions.
       3. **Actual Schema Introspection (`introspector.rs`)**: Introspects live database state (`information_schema`, `pg_indexes`, constraints) filtering system tables.
       4. **Drift & Diff Detection (`diff.rs`)**: Compares actual vs desired under `SafetyPolicy` (`AdditiveOnly` vs `AllowDestructive`) emitting `MigrationStep`s.
       5. **Topological Planner (`planner.rs`)**: Orders DDL execution conforming to dependencies (base tables $\rightarrow$ link tables $\rightarrow$ columns $\rightarrow$ indexes).
       6. **Sea-Query Typed DDL Generator (`executor.rs`)**: Uses `sea_query::Table`, `Index`, `ColumnDef`, and `ForeignKey` typed statements instead of manual string formatting. Executed standalone via `sqlx::AssertSqlSafe`.
  4. **Query & OCC Subsystem (`persistence::query`)**:
     - Implemented `with_occ_retry` catching SQLSTATE `40001` (`serialization_failure`) with randomized exponential backoff and UUIDv4-derived entropy jitter, exhausting to typed `PersistenceError::OccConflictExhausted`.
     - Parameterized wide-table Sea-Query builders (`wide_table.rs`) for visitor queries (`has_pending_draft = FALSE`), pagination, equality filtering (`FieldFilter`), counts, upserts (`ON CONFLICT (id) DO UPDATE SET ...`), and display title lookups.
     - Junction query builders (`junction.rs`) for link inserts (`ON CONFLICT (source_id, target_id) DO UPDATE SET order_index`), deletions, and two-phase batch relation lookups.


---

### 2026-10-26 — Top-Down Code Organization & Test ID Helper Renaming (`create_*`)

- **Context**: Code review revealed inconsistencies in top-down newspaper ordering (high-level orchestrators calling helpers defined far below without intermediate ordering) and misleading names in test support helpers (`test_actor_user`, `test_doc_type_id`, `test_locale_id`, `test_user_id`, `test_attr_id`, `test_service_id`, `test_locales` which create validated newtypes rather than asserting test cases).
- **Decisions**:
  1. **Strict Step-Down Narrative Ordering**: Reorganized 29 files across `domain`, `application`, and `infrastructure` (including `loader.rs`, `executor.rs`, `builder.rs`, `sync.rs`, and `wide_table.rs`) so that callers are immediately followed by their direct callee helpers, reading like a newspaper from highest level of abstraction to lowest level.
  2. **Renamed Test ID Constructors**: Renamed test ID helpers in `domain::test_support::ids` to `create_*` (`create_actor_user`, `create_user_id`, `create_service_id`, `create_doc_type_id`, `create_attr_id`, `create_locales`, `create_locale_id`) reflecting their constructor nature while avoiding naming collisions with `#[test]` test runners.
  3. **Backward Compatibility**: Re-exported `pub use create_* as test_*;` in `domain/src/test_support/ids.rs` for backward compatibility while migrating all internal references across the repository to `create_*`.


---

### 2026-10-26 — Dynamic Schema Migration Engine: Role-Based Clean Architecture Refactoring

- **Context**: Code review of `infrastructure::persistence::migration::dynamic` revealed that `executor.rs` mixed DDL compilation with DB execution, `build_create_table_sql` contained leaky branching for junction tables (`table.is_junction()`), `MigrationStep` was misplaced in `diff.rs`, and table ordering used coarse category bucketing rather than a true dependency graph.
- **Decisions**:
  1. **Role-Based Architecture**:
     - `step.rs`: Pure AST for `MigrationStep` and `SafetyPolicy` (zero external DB/SQL dependencies).
     - `ddl.rs`: Pure compiler translating `MigrationStep` to SQL strings via `sea-query` (zero I/O).
     - `executor.rs`: Pure runtime driver executing statements on `PgPool` outside transactions.
     - `diff.rs`: Pure differ comparing actual vs desired `DatabaseSchema`.
  2. **Generic Primary Key Generation**: Replaced `is_junction()` branching in `build_create_table_sql` with generic table-level `PRIMARY KEY` compilation derived from `is_primary_key` columns. Works uniformly for single-column PKs and composite PKs.
  3. **Streamlined Projection**: Refactored `build_desired_schema` in `builder.rs` into a single loop delegating to `build_document_type_tables`, removing redundant intermediate allocations.
  4. **Kahn's Topological Sort Dependency Graph**: Upgraded `planner.rs` with `DependencyGraph` and Kahn's algorithm sorting `CreateTable` steps by foreign key dependencies (parent before child) and detecting cycles (`PlannerError::CircularDependency`).

---

### 2026-10-27 — RunMode::Service Daemon, Aurora DSQL Connection Factory & Dex OIDC Provider

- **Context**: Completing Phase 4B.3 and Phase 7 of `infrastructure` crate: production runtime daemon, database connection lifecycle for AWS Aurora DSQL and PostgreSQL, and local OIDC identity provisioning.
- **Decisions**:
  1. **Dual-Signal Graceful Shutdown (`RunMode::Service`)**: Implemented HTTP service mode worker in `infrastructure/src/cli/service.rs`. Sets up Axum REST router, binds TCP listener, and uses `axum::serve(listener, router).with_graceful_shutdown()` listening concurrently for `SIGINT` (Ctrl+C) and Unix `SIGTERM` without panicking on registration failures.
  2. **Fail-Fast Auto-Migration**: If `--auto-migrate` / `auto_migrate: true` is configured in service mode, static system migrations and dynamic schema synchronization run prior to binding the TCP listener. Any migration failure immediately aborts startup, preventing traffic on out-of-sync schemas.
  3. **Aurora DSQL & PostgreSQL Connection Factory (`persistence::connection`)**:
     - `detect_connection_target`: Automatically classifies URLs into `ConnectionTarget::Postgres` or `ConnectionTarget::AuroraDsql` based on host pattern (`*.dsql.[region].on.aws`) or `?dsql=true` query flag, extracting cluster endpoint, AWS region, user, and database.
     - `DsqlTokenProvider`: Trait for dynamic IAM authentication tokens with `EnvironmentDsqlTokenProvider` (reading `LUMINAIR_DSQL_AUTH_TOKEN`) and `StaticDsqlTokenProvider`.
     - **Connection Lifetime & TLS**: Enforces `max_lifetime = 10 minutes` (under the 15-minute DSQL IAM token expiry window), `idle_timeout = 5 minutes`, and `ssl_mode = Require` on Aurora DSQL pools.
     - Centralized in `persistence::create_connection_pool` and reused across CLI `migrate`, `dry_run`, and `service` modes.
  4. **Dex OIDC Provider**: Added Dex (`ghcr.io/dexidp/dex:v2.41.1`) to `docker-compose.yml` with `config/dex.yaml` configuring in-memory storage, issuer `http://localhost:5556/dex`, pre-seeded users (`admin@luminair.dev`, `editor@luminair.dev`), and public client `luminair-admin-spa` as a lightweight Keycloak alternative.
