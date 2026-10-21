# Refactoring Plan: Domain and Application Crates (ADR-004 & Domain Review Merge)

- **Target Crates**: `domain`, `application`
- **Governing Architecture**: [`ADR-004`](./adr/ADR-004-draft-and-publish-table-pattern.md) (Asymmetric Hybrid Draft-and-Publish Persistence Pattern) & Findings from Domain/Application Review
- **Execution Mode**: Orchestrated step-by-step via **Gemini Flash** subagents (`Model: "flash"`), reviewed and verified by the orchestrator after each step.
- **Status**: Approved — Ready for Step-by-Step Execution
- **Date**: 2026-10-20

---

## Overview

This document specifies the complete, merged refactoring plan for transitioning the `domain` and `application` crates to support the **Asymmetric Hybrid Persistence Pattern (ADR-004)** while resolving all remaining architectural findings from the **Domain & Application Crate Review**.

### Summary of Already Completed Review Fixes (Baseline)
- **F1 (Owner Permissions)**: Document owners may read, update, and delete their own documents, while publishing/unpublishing strictly requires `Permission::PublishDocument`.
- **F2 (Unpublish Attribution)**: `DocumentInstance::unpublish(by, now)` records the actor and updates `audit.updated_by` and `updated_at`.
- **F3 (Self-Review Prevention)**: `AccessRequest::approve` and `reject` forbid `reviewer_id == requester_id`.
- **Enrollment Invariant**: `AccessRequestPolicy` rejects requests from users who already hold any active role (`DomainError::UserAlreadyEnrolled`).
- **Pre-compiled Regex (`PERF`)**: `RegexPattern` pre-compiles and caches `Arc<Regex>` on construction.
- **Decimal Precision & Scale (`NUM`)**: `PrimitiveValue::matches_primitive_type` validates total significant digits (`precision`) and fractional digits (`scale`).

### Core Architectural Decisions in This Plan
1. **Strongly-Typed Mandatory `ActorId` Attribution**:
   - Every content lifecycle operation (`new`, `touch`, `publish`, `unpublish`, `edit_draft`) and every audit field (`created_by`, `updated_by`, `published_by`, `unpublished_by`) uses a mandatory **`ActorId`** enum (`ActorId::User(UserId) | ActorId::Service(ServiceId) | ActorId::System`) instead of `Option<UserId>`.
   - Serializes to/from compact URN strings (`"user:<id>"`, `"service:<id>"`, `"system"`) for database `VARCHAR(255) NOT NULL` storage.
2. **Schema Metadata (`display_fields`) with Bounded Complexity**:
   - `DocumentTypeInfo` defines `display_fields: Vec<AttributeId>`.
   - Enforces two strict limits at schema registration:
     1. **Maximum 3 display fields total** (`display_fields.len() <= 3`).
     2. **Maximum 1 singular relational display field** (`HasOne` or `BelongsToOne`; multi-relations `HasMany` / `BelongsToMany` are rejected).
   - Domain schema structs (`DocumentType`, `FieldDefinition`, `RelationDefinition`) remain free of Serde derives; JSON config loading and REST `GET` endpoints use dedicated DTOs in `infrastructure`.
3. **Composite Identity & Read-Time 2-Phase Relational Enrichment**:
   - Write-time `extract_display_identity` extracts scalar and localized scalar values into `display_values: IndexMap<AttributeId, String>` and `display_title: String`.
   - Read-time 2-phase batch enrichment in `DocumentsService::list_admin_headers` resolves the (at most 1) singular relational display field in a single batched lookup (`find_display_titles_by_ids`) for the current page (~25 rows).
4. **Aggregate Root Cleanup & State Machine Expansion**:
   - Remove dead leaked database field `db_row_id: Option<DocumentInstanceId>` from `DocumentInstance`.
   - Encapsulate `content.fields` behind `get_field`, `set_field`, `remove_field`, and `fields()` methods.
   - Expand `PublicationState` to `Draft`, `Published`, `Modified`, and `Unpublished`, adding `edit_draft` and `discard_draft`.
5. **Two-Tier Content Validation**:
   - Split validation into **lenient draft validation** (`validate_draft_content`) and **strict publication validation** (`validate_publish_content`).
6. **CQRS Repository Ports & Lifetime Ergonomics**:
   - Relax `type_id: &'static DocumentTypeId` to `type_id: &DocumentTypeId` across `DocumentInstanceRepository`.
   - Replace generic `find_by_type` with decoupled CQRS methods (`find_published`, `list_admin_headers`, `find_display_titles_by_ids`, `find_draft_by_id`, `find_published_by_id`, `save_draft`, `publish`, `unpublish`, `discard_draft`) and `DocumentSnapshotRepository`.

---

## Phase 1: Domain Crate Refactoring (`domain`)

### Step 1.0: Strongly-Typed `ActorId` Attribution
* **Files**: `domain/src/auth/user.rs`, `domain/src/auth/service.rs`, `domain/src/auth/mod.rs`, `domain/src/auth/access_request.rs`, `domain/src/auth/role.rs`
* **Changes**:
  1. Define `ServiceId` using `#[nutype]` (`sanitize(trim)`, `validate(not_empty, len_char_max = 245)`, ensuring `245` + `8` bytes for `"service:"` prefix = `253 <= 255`).
  2. Define `ActorId`:
     ```rust
     #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
     #[serde(tag = "kind", content = "id", rename_all = "snake_case")]
     pub enum ActorId {
         User(UserId),
         Service(ServiceId),
         System,
     }
     ```
  3. Implement `Display` and `ActorId::to_urn(&self) -> String` / `ActorId::from_urn(s: &str) -> Result<Self, DomainError>`:
     - `ActorId::User(u)` $\leftrightarrow$ `"user:<u_str>"`
     - `ActorId::Service(s)` $\leftrightarrow$ `"service:<s_str>"`
     - `ActorId::System` $\leftrightarrow$ `"system"`
  4. Implement `From<UserId>` and `From<ServiceId>` for `ActorId`.
  5. Attribution in Access Requests & Roles:
     - `AccessRequest.reviewed_by` and `UserRoleAssignment.granted_by` use `Option<ActorId>`.
     - `AccessRequest::approve` and `AccessRequest::reject` accept `impl Into<ActorId>`.
  6. Update `AuthorizationService::can` to accept `actor: &ActorId`:
     - If `actor` is `ActorId::System`, allow all actions.
     - Owner rule (`inst.is_owned_by(actor)`) applies when `self.audit.created_by == *actor` for `ReadDocument`, `UpdateDocument`, and `DeleteDocument`.
     - Otherwise evaluates RBAC across `roles`.
* **Verification**:
  - Unit tests for `ActorId` URN roundtrip (`user:...`, `service:...`, `system`), ownership matching, and `AuthorizationService::can` with `User`, `Service`, and `System` actors.

### Step 1.1: Schema Metadata (`display_fields` & Validation Limits)
* **Files**: `domain/src/schema/documents.rs`, `domain/src/schema/registry.rs`, `domain/src/errors.rs`
* **Changes**:
  1. **Add `display_fields` to `DocumentTypeInfo`**:
     - Add `pub display_fields: Vec<AttributeId>` to `DocumentTypeInfo` (no Serde derives on `DocumentType` / `DocumentTypeInfo`).
  2. **Validate `display_fields` at Schema Registration**:
     - Add `DomainError::InvalidSchemaDisplayField { type_id: DocumentTypeId, reason: String }`.
     - In `DocumentType::validate_display_fields(&self) -> Result<(), DomainError>` (called by `SchemaRegistry::register`):
       - **Rule 1 (Max 3 Total)**: `self.info.display_fields.len() <= 3` (reject if $> 3$).
       - **Rule 2 (No Duplicates)**: Every `AttributeId` in `display_fields` must be unique.
       - **Rule 3 (Allowed Field/Relation Types)**: Each `attr_id` must exist either in `self.fields` (excluding `FieldType::Json`) **or** in `self.relations` with `RelationType::HasOne` or `RelationType::BelongsToOne` (`HasMany` and `BelongsToMany` are rejected).
       - **Rule 4 (Max 1 Relational Display Field)**: At most **1** attribute in `display_fields` may reference a relation in `self.relations`.
  3. **Fallback Helper (`effective_display_fields`)**:
     - `DocumentType::effective_display_fields(&self) -> Vec<AttributeId>`: if `info.display_fields` is non-empty, returns it; otherwise returns the first declared `Text`, `LocalizedText`, or `Uid` field in `self.fields` (if any).
  4. **Full Schema Graph Validation (`SchemaRegistry::try_new` & `validate`)**:
     - `SchemaRegistry::try_new(types)` / `SchemaRegistry::validate(&self)` validates the full schema graph across all registered types, including `display_fields` rules, field constraint applicability, and relation target existence.
* **Verification**:
  - Unit tests verifying:
    - Valid `display_fields` with up to 3 fields (including 1 `HasOne` or `BelongsToOne`).
    - Rejection of $> 3$ display fields.
    - Rejection of $> 1$ relational display field.
    - Rejection of `HasMany` / `BelongsToMany` or `FieldType::Json` or undeclared attributes in `display_fields`.

### Step 1.2: Display Extraction Domain Service (Write-Time Scalars)
* **File**: `domain/src/content/values.rs`
* **Changes**:
  1. Add `extract_display_identity(doc_type: &DocumentType, instance_id: DocumentInstanceId, fields: &HashMap<AttributeId, ContentValue>, default_locale: &LocaleId) -> (IndexMap<AttributeId, String>, String)`:
     - Iterates through `doc_type.effective_display_fields()`.
     - For each attribute declared in `doc_type.fields`:
       - Extracts a formatted string representation of the scalar or localized value (using `default_locale` for `LocalizedText`, skipping `Null`).
       - Inserts `(attr_id, formatted_string)` into `IndexMap<AttributeId, String>`, preserving declaration order.
     - Attributes referring to a singular relation (`HasOne` / `BelongsToOne`) are skipped at write time (they are enriched at read time in `list_admin_headers`).
     - Delegates title formatting to `format_display_title(&display_values, instance_id)`.
  2. Add `pub fn format_display_title(display_values: &IndexMap<AttributeId, String>, fallback_id: DocumentInstanceId) -> String`:
     - Joins all non-empty values in declaration order with `" — "`.
     - If all values are empty, falls back to `fallback_id.to_string()`.
     - Truncates output to 255 characters to produce the final `display_title`.
* **Verification**:
  - Unit tests covering single-field, composite 3-field (`terminal-id` + `brand-name` + `removing-date`), null-field omission, localized-field extraction, and empty-fallback to `instance_id`.

### Step 1.3: Aggregate Root Cleanup & State Machine Expansion
* **File**: `domain/src/content/instance.rs`
* **Changes**:
  1. **Remove `db_row_id` (Review Finding 3)**:
     - Delete `pub db_row_id: Option<DocumentInstanceId>` from `DocumentInstance`.
  2. **Mandatory `ActorId` Attribution**:
     - Update `AuditTrail` (`created_by: ActorId`, `updated_by: ActorId`) and `PublicationState` (`published_by: ActorId`, `unpublished_by: ActorId`) to use `ActorId` instead of `Option<UserId>`.
  3. **Encapsulate Field Mutations**:
     - Make `DocumentContent.fields` private (`fields: HashMap<AttributeId, ContentValue>`).
     - Add methods on `DocumentInstance` (and `DocumentContent` getters):
       - `fields(&self) -> &HashMap<AttributeId, ContentValue>`
       - `get_field(&self, attr_id: &AttributeId) -> Option<&ContentValue>`
       - `set_field(&mut self, attr_id: AttributeId, value: ContentValue)`
       - `remove_field(&mut self, attr_id: &AttributeId) -> Option<ContentValue>`
       - `replace_fields(&mut self, fields: HashMap<AttributeId, ContentValue>)`
  4. **Expand `PublicationState`**:
     ```rust
     #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
     pub enum PublicationState {
         Draft {
             last_published_revision: Option<u32>,
         },
         Published {
             revision: u32,
             published_at: DateTime<Utc>,
             published_by: ActorId,
         },
         Modified {
             live_revision: u32,
             published_at: DateTime<Utc>,
             published_by: ActorId,
             draft_version: u32,
         },
         Unpublished {
             last_published_revision: u32,
             unpublished_at: DateTime<Utc>,
             unpublished_by: ActorId,
         },
     }
     ```
  5. **State Machine Methods**:
     - `publish(&mut self, by: ActorId, now: DateTime<Utc>) -> Result<u32, DomainError>`: transitions `Draft`, `Modified`, `Unpublished`, or `Published` to `Published`, increments revision (`last + 1`), and calls `self.touch(by, now)`.
     - `unpublish(&mut self, by: ActorId, now: DateTime<Utc>) -> Result<(), DomainError>`: transitions `Published` or `Modified` to `Unpublished`, preserving `last_published_revision` and recording `unpublished_at` / `unpublished_by`. Rejects `Draft` and `Unpublished` with `DomainError::InvalidStateTransition`.
     - `edit_draft(&mut self, by: ActorId, now: DateTime<Utc>)`: transitions `Published { revision, published_at, published_by }` to `Modified { live_revision: revision, published_at, published_by, draft_version: self.audit.version + 1 }` and calls `self.touch(by, now)`. If already `Draft`, `Modified`, or `Unpublished`, increments draft version and touches audit trail.
     - `discard_draft(&mut self) -> Result<(), DomainError>`: transitions `Modified { live_revision, published_at, published_by, .. }` back to `Published { revision: live_revision, published_at, published_by }`. Rejects calls on `Draft`, `Published`, or `Unpublished` with `DomainError::InvalidStateTransition`.
     - `restore_snapshot(&mut self, snapshot: &DocumentSnapshot, actor: ActorId, now: DateTime<Utc>) -> Result<(), DomainError>`: restores content and relations from historical snapshot, transitions to `Modified`, and updates audit trail.
  6. **Reconstitution for Persistence Hydration**:
     - `pub fn reconstitute(id: DocumentInstanceId, type_id: DocumentTypeId, content: DocumentContent, relations: HashMap<AttributeId, Vec<ResolvedRelation>>, audit: AuditTrail) -> Self`: constructs a `DocumentInstance` directly from repository storage state without triggering mutation events or bumping versions.
* **Verification**:
  - Update inline unit tests and case table tests in `domain/tests/lc_document_lifecycle.rs` to cover `LC-01` through `LC-12`.

### Step 1.4: Lightweight Projections (`DocumentHeader` & `DocumentSnapshot`)
* **File**: `domain/src/content/header.rs` (re-exported in `domain/src/content/mod.rs`)
* **Changes**:
  1. Define `DocumentStatus`:
     ```rust
     #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
     #[serde(rename_all = "snake_case")]
     pub enum DocumentStatus {
         Draft,
         Published,
         Modified,
         Unpublished,
     }
     ```
  2. Define `DocumentHeader`:
     ```rust
     #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
     pub struct DocumentHeader {
         pub id: DocumentInstanceId,
         pub document_type_id: DocumentTypeId,
         pub display_values: IndexMap<AttributeId, String>,
         pub display_title: String,
         /// Target ID for the (at most 1) singular relational display field, used for Phase 2 read-time enrichment
         pub relational_display_target: Option<(AttributeId, DocumentInstanceId)>,
         pub status: DocumentStatus,
         pub slug: Option<String>,
         pub revision: Option<u32>,
         pub draft_version: Option<u32>,
         pub created_at: DateTime<Utc>,
         pub created_by: ActorId,
         pub updated_at: DateTime<Utc>,
         pub updated_by: ActorId,
         pub published_at: Option<DateTime<Utc>>,
         pub published_by: Option<ActorId>,
     }
     ```
  3. Define `SnapshotId` (UUIDv7 `nutype`), `SnapshotReason` (`Publish`, `Checkpoint`, `Unpublish`), `DocumentSnapshot`, and `SnapshotHeader` (with `created_by: ActorId`).
  4. Implement `DocumentHeader::enrich_relational_display(&mut self, effective_fields: &[AttributeId], rel_attr: &AttributeId, target_title: String)`:
     - Merges `target_title` into `display_values` under `rel_attr`.
     - Rebuilds `display_values` in the canonical order of `effective_fields`.
     - Recomputes `display_title` via `format_display_title` and clears `relational_display_target`.

### Step 1.5: Two-Tier Content Validation
* **File**: `domain/src/content/validator.rs`
* **Changes**:
  1. `validate_draft_content(doc_type, content, config) -> Result<(), Vec<DomainError>>`:
     - Checks scalar type compatibility (`matches_field_type`), locale validity, and field constraints for present non-null fields.
     - Rejects undeclared attributes (`DomainError::UnknownAttribute`).
     - Skips `required: true` checks for missing/null fields so incomplete work-in-progress drafts can be saved, while enforcing type compatibility, locale validity, and field constraints on present non-null values.
  2. `validate_publish_content(doc_type, content, config) -> Result<(), Vec<DomainError>>`:
     - Enforces all strict schema rules (`required: true`, `FieldConstraint` bounds/lengths/regex, and locale checks).
     - Keep `validate_content` as an alias or delegate to `validate_publish_content` for backward compatibility with existing tests.

### Step 1.6: Repository Ports Refactoring & Lifetime Ergonomics
* **File**: `domain/src/content/ports.rs`
* **Changes**:
  1. **Relax `'static` Lifetime on `DocumentTypeId` (Review Finding 1)**:
     - Change `type_id: &'static DocumentTypeId` to `type_id: &DocumentTypeId` across all methods on `DocumentInstanceRepository`.
  2. **Expand `DocumentInstanceRepository` with CQRS & Lifecycle Methods (Legacy `save` removed)**:
     - The legacy `save` method is removed in favor of explicit lifecycle-specific persistence.
     - `find_published(type_id: &DocumentTypeId, pagination: Pagination, filters: Vec<FieldFilter>) -> Future<Result<Page<DocumentInstance>, DomainError>>`
     - `find_published_by_id(type_id: &DocumentTypeId, id: DocumentInstanceId) -> Future<Result<Option<DocumentInstance>, DomainError>>`
     - `find_draft_by_id(type_id: &DocumentTypeId, id: DocumentInstanceId) -> Future<Result<Option<DocumentInstance>, DomainError>>`
     - `list_admin_headers(type_id: &DocumentTypeId, pagination: Pagination, status_filter: Option<DocumentStatus>) -> Future<Result<Page<DocumentHeader>, DomainError>>`
     - `find_display_titles_by_ids(type_id: &DocumentTypeId, ids: &[DocumentInstanceId]) -> Future<Result<HashMap<DocumentInstanceId, String>, DomainError>>`
     - `save_draft(instance: &DocumentInstance, display_values: &IndexMap<AttributeId, String>, display_title: &str) -> Future<Result<(), DomainError>>`
     - `publish(instance: &DocumentInstance, revision: u32, display_values: &IndexMap<AttributeId, String>, display_title: &str, snapshot: &DocumentSnapshot) -> Future<Result<(), DomainError>>`
     - `unpublish(instance: &DocumentInstance, display_values: &IndexMap<AttributeId, String>, display_title: &str, snapshot: &DocumentSnapshot) -> Future<Result<(), DomainError>>`
     - `discard_draft(type_id: &DocumentTypeId, id: DocumentInstanceId) -> Future<Result<Option<DocumentInstance>, DomainError>>`
     - `delete(type_id: &DocumentTypeId, id: DocumentInstanceId) -> Future<Result<(), DomainError>>`
     - `count(type_id: &DocumentTypeId, filters: Vec<FieldFilter>) -> Future<Result<u64, DomainError>>`
     - `fetch_relations(...)` and `exists_for_type(type_id: &DocumentTypeId)`
  3. **Introduce `DocumentSnapshotRepository`**:
     - `save_snapshot(snapshot: &DocumentSnapshot) -> Future<Result<(), DomainError>>`
     - `list_snapshots(type_id: &DocumentTypeId, doc_id: DocumentInstanceId) -> Future<Result<Vec<SnapshotHeader>, DomainError>>`
     - `get_snapshot(type_id: &DocumentTypeId, doc_id: DocumentInstanceId, revision: u32) -> Future<Result<Option<DocumentSnapshot>, DomainError>>`

---

## Phase 2: Application Crate Refactoring (`application`)

### Step 2.1: CallerContext & Use Case Commands Expansion
* **Files**: `application/src/context.rs`, `application/src/commands/documents.rs`, `application/src/errors.rs`
* **Changes**:
  1. Update `CallerContext` to hold `pub actor: ActorId` and `pub roles: Vec<Role>`:
     - `CallerContext::new(actor: impl Into<ActorId>, roles: Vec<Role>)`
     - `CallerContext::service(service_id: ServiceId, roles: Vec<Role>)`
     - `CallerContext::system()` uses `ActorId::System` (eliminating the hardcoded `"system-internal"` fake `UserId`).
  2. Add commands in `application/src/commands/documents.rs`:
     - `DiscardDraftCommand`, `RestoreSnapshotCommand`, `ListAdminHeadersCommand`, `ListSnapshotsCommand`.
     - Add optional `expected_version: Option<u32>` to `PublishDocumentCommand`.

### Step 2.2: Application Service Orchestration (`DocumentsService`)
* **File**: `application/src/services/documents.rs`
* **Changes**:
  1. **Generic ISP Decoupling (`DocumentsServiceImpl<R, S = R>`)**:
     - Parameterized over `R: DocumentInstanceRepository` and `S: DocumentSnapshotRepository` (defaulting `S = R`).
     - `DocumentsServiceImpl::new(instance_repo: R, context: &'static SystemContext) -> Self` for unified repos.
     - `DocumentsServiceImpl::new_with_snapshots(instance_repo: R, snapshot_repo: S, context: &'static SystemContext) -> Self` for decoupled snapshot storage.
  2. **`create` use case**:
     - Computes `(display_values, display_title)` via `extract_display_identity`.
     - If `draft_and_publish: true`: runs `validate_draft_content`, calls `repo.save_draft(&instance, &display_values, &display_title)`.
     - If `doc_type.options.draft_and_publish == false`: runs `validate_publish_content`, immediately transitions the instance via `instance.publish(caller.actor.clone(), now)`, constructs a `DocumentSnapshot` (`SnapshotReason::Publish`), and persists atomically via `repo.publish(&instance, 1, &display_values, &display_title, &snapshot)`.
  3. **`update` use case**:
     - Loads working copy, calls `instance.edit_draft(caller.actor.clone(), Utc::now())`, updates fields/relations, recomputes `(display_values, display_title)`.
     - If `draft_and_publish: true`: runs `validate_draft_content`, saves via `repo.save_draft(...)`.
     - If `doc_type.options.draft_and_publish == false`: runs `validate_publish_content`, immediately advances revision via `instance.publish(caller.actor.clone(), now)`, constructs a `DocumentSnapshot` (`SnapshotReason::Publish`), and persists atomically via `repo.publish(&instance, rev, &display_values, &display_title, &snapshot)`.
  4. **`publish` use case**:
     - Verifies `Permission::PublishDocument`.
     - Verifies `expected_version` (if provided) matches `instance.audit.version`.
     - Runs strict `validate_publish_content`.
     - Transitions via `instance.publish(caller.actor.clone(), now)`, creates `DocumentSnapshot` (`SnapshotReason::Publish`), and persists atomically via `repo.publish(&instance, rev, &display_values, &display_title, &snapshot)`.
  5. **`unpublish` use case**:
     - Verifies `Permission::PublishDocument`.
     - Transitions via `instance.unpublish(caller.actor.clone(), now)`, creates `DocumentSnapshot` (`SnapshotReason::Unpublish`), and persists atomically via `repo.unpublish(&instance, &display_values, &display_title, &snapshot)`.
  5. **`discard_draft` use case (`DS-13`)**:
     - Verifies `Permission::UpdateDocument` or document ownership.
     - Loads draft, calls `instance.discard_draft()`, and invokes `repo.discard_draft(type_id, id)` to restore clean `Published` state.
  6. **`restore_snapshot` use case (`DS-14`)**:
     - Verifies `Permission::UpdateDocument` or ownership.
     - Loads historical snapshot by `(type_id, doc_id, revision)` from `snapshot_repo`.
     - Copies snapshot `fields` and `relations` into the instance, calls `instance.edit_draft(caller.actor.clone(), now)`, recomputes `(display_values, display_title)`, and saves via `repo.save_draft(...)`.
  7. **`list_admin_headers` use case with 2-Phase Relational Enrichment (`DS-15`)**:
     - Verifies `Permission::ReadDocument`.
     - **Phase 1**: Calls `repo.list_admin_headers(type_id, pagination, status_filter)` to get `Page<DocumentHeader>`.
     - **Phase 2 (At Most 1 Batched Query)**:
       - If `doc_type.effective_display_fields()` includes a singular relation (`rel_attr`), collects all `target_id`s from `header.relational_display_target` across the page.
       - Calls `repo.find_display_titles_by_ids(&rel_def.target, &target_ids)` once.
       - Rebuilds `header.display_values` in the exact declaration order of `effective_display_fields()` (inserting the resolved related title at `rel_attr`) and updates `header.display_title`.

### Step 2.3: Fake Repositories & Declarative Spec Tests
* **Files**: `application/src/test_support.rs`, `application/tests/ds_documents_service.rs`, `domain/tests/lc_document_lifecycle.rs`
* **Changes**:
  1. Update `FakeDocumentInstanceRepository` and add `FakeDocumentSnapshotRepository` in `application/src/test_support.rs`.
  2. Implement spec tests `LC-09` through `LC-12` in `domain/tests/lc_document_lifecycle.rs` and `DS-13` through `DS-15` (including 2-phase relational display enrichment) in `application/tests/ds_documents_service.rs`.
  3. Run `cargo clippy --workspace --tests -- -D warnings` and `cargo test --workspace` to verify 100% green build and spec traceability.
