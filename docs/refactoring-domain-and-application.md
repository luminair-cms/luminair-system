# Refactoring Plan: Domain and Application Crates (ADR-004 Asymmetric Persistence)

- **Target Crates**: `domain`, `application`
- **Governing Architecture**: [`ADR-004`](./adr/ADR-004-draft-and-publish-table-pattern.md) (Asymmetric Hybrid Draft-and-Publish Persistence Pattern)
- **Status**: Ready for implementation (Phase-by-Phase Execution Guide)
- **Date**: 2026-10-09

---

## Overview

This document specifies the exact refactoring plan for transitioning the existing in-memory domain model and application orchestration services to support the Asymmetric Hybrid Persistence Pattern.

### Objectives
1. Introduce composite identity via `display_fields: Vec<AttributeId>` and materialized `IndexMap<AttributeId, String>`.
2. Expand `PublicationState` to support `Modified` (published with pending draft changes) and `Unpublished` states.
3. Clean up aggregate internals: eliminate leaked infrastructure surrogate key `db_row_id` and encapsulate field mutations.
4. Split all-or-nothing validation into **lenient draft validation** and **strict publication validation**.
5. Decouple repository ports under CQRS principles (`find_published` vs. `list_admin_headers` vs. `find_draft_by_id`).
6. Add commands and use cases for `DiscardDraft`, `RestoreSnapshot`, and `ListAdminHeaders`.

---

## Phase 1: Domain Crate Refactoring (`domain`)

### Step 1.1: Schema Metadata Enhancement
* **File**: `domain/src/schema/documents.rs`
* **Changes**:
  1. Add `display_fields: Vec<AttributeId>` to `DocumentTypeInfo`.
  2. Implement validation in `DocumentTypeRegistry`: verify that every `AttributeId` in `display_fields` is declared in `doc_type.fields`.
  3. Provide fallback: if `display_fields` is empty, fallback automatically to the first declared `Text` field, or `"id"`.
* **Verification**: Add unit test `test_display_fields_validated_against_declared_attributes`.

### Step 1.2: Display Extraction Domain Service
* **File**: `domain/src/content/values.rs`
* **Changes**:
  1. Add `extract_display_identity(doc_type, fields, locale) -> (IndexMap<AttributeId, String>, String)`:
     - Iterates through `doc_type.info.display_fields`.
     - Extracts string representations of scalar values.
     - Preserves insertion order using `IndexMap<AttributeId, String>`.
     - Formats joined fallback `display_title` (e.g. `"POS-042 — Nike Store — 2026-05-12"`).
* **Verification**: Add unit test testing single-field, composite-field, null-field, and localized-field extractions.

### Step 1.3: Aggregate Root Cleanup & State Machine Expansion
* **File**: `domain/src/content/instance.rs`
* **Changes**:
  1. **Remove `db_row_id: Option<DocumentInstanceId>`**: Eliminate this leaked database surrogate ID entirely from the domain aggregate.
  2. **Encapsulate Field Mutations**: Make `content.fields` private. Add aggregate methods:
     - `set_field(&mut self, attr_id: AttributeId, value: ContentValue)`
     - `get_field(&self, attr_id: &AttributeId) -> Option<&ContentValue>`
  3. **Expand `PublicationState`**:
     ```rust
     #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
     pub enum PublicationState {
         Draft {
             last_published_revision: Option<u32>,
         },
         Published {
             revision: u32,
             published_at: DateTime<Utc>,
             published_by: Option<UserId>,
         },
         Modified {
             live_revision: u32,
             published_at: DateTime<Utc>,
             published_by: Option<UserId>,
             draft_version: u32,
         },
         Unpublished {
             last_published_revision: u32,
             unpublished_at: DateTime<Utc>,
             unpublished_by: Option<UserId>,
         },
     }
     ```
  4. **State Machine Methods**:
     - `publish(by, now) -> Result<u32, DomainError>`: transitions `Draft` or `Modified` to `Published`, increments revision, updates audit.
     - `unpublish(by, now) -> Result<(), DomainError>`: transitions `Published` or `Modified` to `Unpublished`, preserves revision.
     - `edit_draft(by, now)`: transitions `Published` to `Modified`, increments draft version and touches audit.
     - `discard_draft() -> Result<(), DomainError>`: transitions `Modified` back to clean `Published`. Rejects calls on pure `Draft`.
* **Verification**: Update case table tests in `domain/tests/lc_document_lifecycle.rs` to cover `LC-09` through `LC-12`.

### Step 1.4: Lightweight Projections (`DocumentHeader` & `DocumentSnapshot`)
* **File**: `domain/src/content/instance.rs` or `domain/src/content/header.rs`
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
         pub status: DocumentStatus,
         pub slug: Option<String>,
         pub revision: Option<u32>,
         pub draft_version: Option<u32>,
         pub created_at: DateTime<Utc>,
         pub created_by: Option<UserId>,
         pub updated_at: DateTime<Utc>,
         pub updated_by: Option<UserId>,
         pub published_at: Option<DateTime<Utc>>,
         pub published_by: Option<UserId>,
     }
     ```
  3. Define `DocumentSnapshot` and `SnapshotHeader`.

### Step 1.5: Two-Tier Content Validation
* **File**: `domain/src/content/validator.rs`
* **Changes**:
  1. `validate_draft_content(doc_type, content, config) -> Result<(), Vec<DomainError>>`:
     - Checks scalar type compatibility (e.g. integer does not contain string).
     - Rejects undeclared attributes.
     - Skips missing required fields and allows unresolved draft relations.
  2. `validate_publish_content(doc_type, content, config) -> Result<(), Vec<DomainError>>`:
     - Runs existing strict validation (enforces `required`, length, bounds, regex).
     - Validates relation targets.

### Step 1.6: Repository Ports Refactoring
* **File**: `domain/src/content/ports.rs`
* **Changes**:
  1. Replace generic `find_by_type` with decoupled methods:
     - `find_published`: queries wide table with `FieldFilter` and pagination.
     - `list_admin_headers`: queries unified drafts and wide table with `DocumentStatusFilter`.
     - `find_draft_by_id`: loads working draft JSONB by ID.
     - `find_published_by_id`: loads published document by ID.
     - `save_draft`, `publish`, `unpublish`, `discard_draft`.
  2. Introduce `DocumentSnapshotRepository`:
     - `list_snapshots(type_id, doc_id) -> Future<Vec<SnapshotHeader>>`
     - `get_snapshot(snapshot_id) -> Future<Option<DocumentSnapshot>>`

### Step 1.7: Test Support & Fake Repository Updates
* **File**: `domain/src/test_support/mod.rs` (or `fake.rs`)
* **Changes**:
  1. Update `FakeDocumentInstanceRepository` to implement the new port methods.
  2. Maintain separate in-memory storage for drafts (`HashMap<(DocumentTypeId, DocumentInstanceId), DocumentInstance>`) and published documents (`HashMap<(DocumentTypeId, DocumentInstanceId), DocumentInstance>`).
  3. Verify `domain/tests/spec_traceability.rs` passes.

---

## Phase 2: Application Layer Refactoring (`application`)

### Step 2.1: Use Case Commands Expansion
* **File**: `application/src/commands/documents.rs`
* **Changes**:
  1. Add `FindPublishedCommand { document_type, pagination, filters, populate }`.
  2. Add `ListAdminHeadersCommand { document_type, pagination, status_filter }`.
  3. Add `DiscardDraftCommand { document_type, document_instance_id }`.
  4. Add `RestoreSnapshotCommand { document_type, document_instance_id, revision }`.
  5. Add `ListSnapshotsCommand { document_type, document_instance_id }`.
  6. Update `PublishDocumentCommand` to include `expected_version: Option<u32>` for OCC safety.

### Step 2.2: Application Service Orchestration (`DocumentsService`)
* **File**: `application/src/services/documents.rs`
* **Changes**:
  1. **`create` use case**:
     - If `draft_and_publish: true`: runs `validate_draft_content`, calls `repo.save_draft(instance)`.
     - If `draft_and_publish: false`: runs `validate_publish_content`, calls `repo.publish(instance)`.
  2. **`update` use case**:
     - If `draft_and_publish: true`: loads working draft via `find_draft_by_id`, mutates fields, runs `validate_draft_content`, calls `repo.save_draft(instance)` (state becomes `Modified` if previously published).
     - If `draft_and_publish: false`: updates directly in wide table via `repo.publish(instance)`.
  3. **`publish` use case**:
     - Validates caller possesses `Permission::PublishDocument`.
     - Loads draft, verifies `expected_version`.
     - Executes strict validation via `validate_publish_content`.
     - Invokes `repo.publish(...)`.
     - Emits `DocumentPublished` event.
  4. **`unpublish` use case**:
     - Validates caller possesses `Permission::PublishDocument`.
     - Invokes `repo.unpublish(...)`.
     - Emits `DocumentUnpublished` event.
  5. **`discard_draft` use case**:
     - Validates caller possesses `Permission::UpdateDocument` or ownership.
     - Invokes `repo.discard_draft(...)`.
  6. **`restore_snapshot` use case**:
     - Loads historical snapshot from snapshot repo.
     - Copies snapshot fields into a working draft.
     - Saves draft via `repo.save_draft(...)`.
  7. **`list_admin_headers` use case**:
     - Validates caller possesses `Permission::ReadDocument`.
     - Invokes `repo.list_admin_headers(...)`.

### Step 2.3: Application Unit & Spec Integration Tests
* **File**: `application/tests/ds_documents_service.rs`
* **Changes**:
  1. Add tests for `DS-13` (Discard Draft), `DS-14` (Restore Snapshot), and `DS-15` (List Admin Headers).
  2. Update existing tests `DS-06` through `DS-09` to verify draft store and published table interactions.
