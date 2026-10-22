# Refactoring Plan: Archive-on-Replace Snapshots, Strapi Unpublish Resolution, & Snapshot Restores

> [!NOTE]
> **Historical Implementation Plan**: This document records the executed plan for archive-on-replace snapshots and unpublish draft resolution. Note: The proposal in Section 1.3 for automatic checkpoints on restore was superseded by the `discard_pending_edits` explicit confirmation guard (preventing silent draft overwrites without creating spurious checkpoints). For current normative specifications, refer to [`docs/content-lifecycle.md`](../content-lifecycle.md), [`docs/design-infrastructure-persistence.md`](../design-infrastructure-persistence.md), and [`docs/api.md`](../api.md).

- **Target Crates**: `domain`, `application`
- **Governing Architecture**: [`ADR-004`](../adr/ADR-004-draft-and-publish-table-pattern.md), [`docs/design-infrastructure-persistence.md`](../design-infrastructure-persistence.md)
- **Status**: Implemented (Checkpoint proposal superseded by `discard_pending_edits` guard)
- **Date**: 2026-10-21

---

## 1. Architectural Goal

Align `domain` and `application` with the finalized snapshot & unpublish semantics in [`ADR-004`](./adr/ADR-004-draft-and-publish-table-pattern.md) and [`docs/design-infrastructure-persistence.md`](./design-infrastructure-persistence.md):

1. **Archive-on-Replace During `publish` (`SnapshotReason::Publish`)**:
   - `document_snapshots` stores **historical revisions** that are no longer the active live row in `[type]` (eliminating duplicate storage of the current live revision in both `[type]` and `document_snapshots`).
   - On the **first publish** (`revision = 1`, when no live row exists in `[type]`): the draft moves to `[type]` and `archived_snapshot` is `None`.
   - On a **subsequent publish** (`revision = N` replacing a live `revision = N - 1` in `[type]`): the previous live row from `[type]` (`VersionView::Published`) is captured as `Some(DocumentSnapshot { revision: N - 1, reason: SnapshotReason::Publish, .. })` and archived atomically inside `DocumentInstanceRepository::publish`.

2. **Strapi Unpublish Resolution (`UnpublishDraftResolution`) & Live Archival (`SnapshotReason::Unpublish`)**:
   - `unpublish` takes the document **offline** (deletes the row from `[type]` so public queries return `None`).
   - The live published row that was in `[type]` (`VersionView::Published`) is archived into `document_snapshots` with `reason: SnapshotReason::Unpublish`.
   - When the document is in `Modified` state (both a live row in `[type]` and pending edits in `document_drafts`), `UnpublishDocumentCommand` supports `UnpublishDraftResolution`:
     - `UnpublishDraftResolution::KeepDraft` (`#[default]`): Keeps the working draft's `fields` and `relations` in `document_drafts` and transitions its state to `PublicationState::Unpublished`.
     - `UnpublishDraftResolution::ReplaceWithPublished`: Replaces the working draft's `fields` and `relations` with the live published content & relations from `[type]` and transitions its state to `PublicationState::Unpublished`.

3. **Automatic `'checkpoint'` Backup on `restore_snapshot` (`SnapshotReason::Checkpoint`)**:
   - When `restore_snapshot` is called on a document that is currently in `PublicationState::Modified { draft_version, .. }` (meaning the user has uncommitted draft edits in `document_drafts`), `DocumentsServiceImpl::restore_snapshot` automatically saves the current working draft to `document_snapshots` with `reason: SnapshotReason::Checkpoint` (`revision: draft_version`) before overwriting the working draft with the restored historical snapshot.
   - Also allow `restore_snapshot` when the document is in `Unpublished` state (transitioning/staying in `Unpublished` with the restored content, or `Modified` when live).

---

## 2. Step-by-Step Implementation Plan

### Step 1: Domain Layer (`domain`)

1. **`domain/src/content/header.rs`**:
   - Define `UnpublishDraftResolution`:
     ```rust
     /// Specifies how pending draft changes should be handled when unpublishing a `Modified` document.
     #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
     #[serde(rename_all = "snake_case")]
     pub enum UnpublishDraftResolution {
         /// Keep the in-progress draft content in `document_drafts` (default).
         #[default]
         KeepDraft,
         /// Replace the draft content in `document_drafts` with the previously published live content.
         ReplaceWithPublished,
     }
     ```
2. **`domain/src/content/ports.rs`**:
   - Update `DocumentInstanceRepository::publish` to accept `archived_snapshot: Option<&DocumentSnapshot>` instead of `snapshot: &DocumentSnapshot`:
     ```rust
     /// Publishes the document instance to the wide table, clears any pending draft row,
     /// and atomically archives the superseded live revision (if any) to `document_snapshots`.
     fn publish(
         &self,
         instance: &DocumentInstance,
         revision: u32,
         display_values: &IndexMap<AttributeId, String>,
         display_title: &str,
         archived_snapshot: Option<&DocumentSnapshot>,
     ) -> impl Future<Output = Result<(), DomainError>> + Send;
     ```

### Step 2: Application Commands (`application/src/commands/documents.rs`)

1. Re-export `pub use domain::content::UnpublishDraftResolution;`.
2. Add `pub draft_resolution: UnpublishDraftResolution` to `UnpublishDocumentCommand`:
   - Default to `UnpublishDraftResolution::KeepDraft` in `UnpublishDocumentCommand::new`.
   - Add builder `.with_draft_resolution(mut self, resolution: UnpublishDraftResolution) -> Self`.

### Step 3: Application Service (`application/src/services/documents.rs`) & Fake Repo (`application/src/test_support.rs`)

1. **`FakeDocumentInstanceRepository::publish` (`application/src/test_support.rs`)**:
   - Accept `archived_snapshot: Option<&DocumentSnapshot>`.
   - Only push to `self.snapshots` when `if let Some(snap) = archived_snapshot`.
2. **`DocumentsServiceImpl::create` & `update` (when `draft_and_publish: false`)**:
   - On `create` (`draft_and_publish: false`): no previous published version exists, so pass `None` for `archived_snapshot` in `instance_repo.publish(...)`.
   - On `update` (`draft_and_publish: false`): capture the previous live `instance` state (before applying the update) into `prev_snapshot = DocumentSnapshot { revision: prev_rev, reason: SnapshotReason::Publish, .. }` and pass `Some(&prev_snapshot)` to `instance_repo.publish(...)`.
3. **`DocumentsServiceImpl::publish` (when `draft_and_publish: true`)**:
   - Load the existing live published document (if any) via `self.instance_repo.find_by_id(cmd.document_type, cmd.document_instance_id, VersionView::Published).await?`.
   - If `Some(prev_live)` exists (with `PublicationState::Published { revision: prev_rev, .. }`), construct `archived_snapshot = Some(DocumentSnapshot { revision: prev_rev, fields: prev_live.fields().clone(), relations: prev_live.relations.clone(), reason: SnapshotReason::Publish, .. })`; otherwise `None`.
   - Transition working `instance.publish(...)` and call `self.instance_repo.publish(&instance, rev, &display_values, &display_title, archived_snapshot.as_ref()).await?`.
4. **`DocumentsServiceImpl::unpublish`**:
   - Load both the working instance (`VersionView::Working`) and the live published instance (`VersionView::Published`).
   - Construct the `'unpublish'` snapshot from the **live published instance** (`live_inst`) so the exact published revision leaving `[type]` is archived in `document_snapshots` (`reason: SnapshotReason::Unpublish`).
   - If `cmd.draft_resolution == UnpublishDraftResolution::ReplaceWithPublished`, replace `instance`'s fields and relations with `live_inst.fields().clone()` and `live_inst.relations.clone()`.
   - Call `instance.unpublish(caller.actor.clone(), now)?`, compute `(display_values, display_title)`, and call `self.instance_repo.unpublish(&instance, &display_values, &display_title, &snapshot).await?`.
5. **`DocumentsServiceImpl::restore_snapshot`**:
   - Before calling `instance.restore_snapshot(&snapshot, ...)`, check if `matches!(instance.content.publication_state, PublicationState::Modified { .. })`:
     - If `Modified { draft_version, .. }`, construct a checkpoint `DocumentSnapshot` (`revision: draft_version`, `fields: instance.fields().clone()`, `relations: instance.relations.clone()`, `reason: SnapshotReason::Checkpoint`) and save it via `self.snapshot_repo.save_snapshot(&checkpoint).await?`.
   - Then restore the target snapshot and persist via `save_draft` (or `publish` if `draft_and_publish: false`).

### Step 4: Spec & Integration Tests (`application/tests/ds_documents_service.rs` & `docs/specs/application/documents-service.md`)

1. Update `ds_14_restore_snapshot_loads_historical_revision_to_draft`:
   - Publish V1 (`rev 1`), update & publish V2 (`rev 2` $\rightarrow$ archives V1 as `SnapshotReason::Publish` in `document_snapshots`).
   - Edit draft to `"V3 Unsaved Draft"` (`Modified` state).
   - Call `restore_snapshot` for `revision: 1`:
     - Verify `"V3 Unsaved Draft"` was automatically saved as a `SnapshotReason::Checkpoint` snapshot in `list_snapshots`!
     - Verify working copy now has `"V1 Title"` while live published copy still has `"V2 Title"`.
2. Update `ds_08_unpublish_transitions_to_draft_with_last_revision` (and add test for `UnpublishDraftResolution::KeepDraft` vs `ReplaceWithPublished`):
   - Verify that unpublishing a `Modified` document with `KeepDraft` keeps the draft edits in `VersionView::Working` while archiving the live published version to `document_snapshots` (`SnapshotReason::Unpublish`).
   - Verify that unpublishing a `Modified` document with `ReplaceWithPublished` replaces `VersionView::Working` with the live published content.
3. Run `cargo clippy --offline --workspace --tests -- -D warnings` and `cargo test --offline --workspace`.
