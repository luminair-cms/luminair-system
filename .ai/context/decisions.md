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




