# DS — Documents Application Service Specification

- **Prefix**: `DS`
- **Layer**: `application`
- **Unit under test**: `application::services::DocumentsService`
- **Format**: `U` (use case rules)
- **Test target**: `application/tests/ds_documents_service.rs`

---

## Vocabulary

| Term | Meaning / API symbol | Concrete representation |
|---|---|---|
| `Singleton` (SingleType) | Document type with `DocumentKind::Singleton` | Maximum 1 instance permitted |
| `Collection` | Document type with `DocumentKind::Collection` | Arbitrary number of instances |
| `DraftPublishOn` | Document type with `draft_and_publish: true` | Publish & unpublish enabled |
| `DraftPublishOff` | Document type with `draft_and_publish: false` | Publish & unpublish rejected |
| `OwnerCaller` | Caller whose actor matches `instance.audit.created_by` | `instance.is_owned_by_actor(&caller.actor)` |
| `AdminCaller` | Caller possessing all system permissions | `CallerContext::system()` |
| `UnauthorizedCaller` | Caller without required permission or ownership | Triggers `ApplicationError::Unauthorized` |
| `VersionView::Published` | Default view for `FindByIdCommand`: returns strictly the live published wide table row | `VersionView::Published` |
| `VersionView::Working` | Working draft view for `FindByIdCommand`: returns working draft from draft store or falls back to wide table | `VersionView::Working` |

---

## Rules

### DS-01 — Create Collection Draft Success
- **Status**: `Approved`
- **Origin**: `adr:002`
- **Given**: A valid collection document type in schema registry, and a caller with `CreateDocument` permission.
- **When**: Caller invokes `service.create(&caller, CreateDocumentCommand::new(type_id, valid_fields)).await`.
- **Then**: Returns `Ok(instance)` with `instance.audit.version == 1`, `instance.audit.created_by == caller.actor`, `instance.content.publication_state` as `Draft`, and instance is retrievable from repository.

### DS-02 — Create Blocked for Second SingleType Instance
- **Status**: `Approved`
- **Origin**: `adr:002` (SingleType Guard Option C)
- **Given**: A singleton document type in schema registry, and an existing instance already saved in the repository.
- **When**: Any caller invokes `service.create(&caller, CreateDocumentCommand::new(type_id, valid_fields)).await`.
- **Then**: Returns `Err(ApplicationError::Domain(DomainError::SingleTypeAlreadyExists(type_id)))`.

### DS-03 — Create Rejects Content Violating Field Constraints
- **Status**: `Approved`
- **Origin**: `doc:domain::content::validate_content`
- **Given**: A document type with a required field `title` having `MinLength(5)`.
- **When**: Caller provides fields with `title` shorter than 5 characters or missing.
- **Then**: Returns `Err(ApplicationError::Validation(errors))` containing the failure reason; no instance is saved to the repository.

### DS-04 — Update Bumps Version and Touches Audit Trail
- **Status**: `Approved`
- **Origin**: `adr:002`
- **Given**: An existing document instance with `audit.version == 1`.
- **When**: Authorized caller updates a field via `service.update(&caller, UpdateDocumentCommand::new(instance_id, type_id, updated_fields)).await`.
- **Then**: Returns `Ok(updated)` with `updated.audit.version == 2`, `updated.audit.updated_by == caller.actor`, and updated fields are persisted in the repository.

### DS-05 — Update Retains Unmentioned Fields and Clears Null Fields
- **Status**: `Approved`
- **Origin**: `observed` (PATCH semantics)
- **Given**: An instance holding optional fields `f1 = "val1"` and `f2 = "val2"`.
- **When**: Caller updates with `cmd.fields` containing only `f2: ContentValue::Null`.
- **Then**: `f1` retains `"val1"`, `f2` becomes `ContentValue::Null` (or removed), and other schema constraints pass.

### DS-06 — Publish Advances Revision on DraftPublish Enabled Type
- **Status**: `Approved`
- **Origin**: `adr:004`
- **Given**: An existing draft instance of a `DraftPublishOn` document type, and a caller with `PublishDocument` permission.
- **When**: Caller invokes `service.publish(&caller, PublishDocumentCommand::new(instance_id, type_id)).await`.
- **Then**: Returns `Ok(published)` with `publication_state` as `Published { revision: 1, .. }`, version incremented, and saved in repository.

### DS-07 — Publish Rejects Document of DraftPublish Disabled Type
- **Status**: `Approved`
- **Origin**: `adr:004`
- **Given**: A document type with `draft_and_publish: false` (`DraftPublishOff`).
- **When**: A caller creates an instance (or updates it), or explicitly calls `publish` / `unpublish`.
- **Then**: `create` immediately publishes at `revision: 1` and persists to the wide table, `update` immediately advances to `revision: 2`, while explicit calls to `service.publish(&caller, PublishDocumentCommand::new(instance_id, type_id))` or `service.unpublish(&caller, UnpublishDocumentCommand::new(instance_id, type_id))` return `Err(ApplicationError::Conflict { code: ConflictCode::DraftAndPublishDisabled, .. })` containing `"does not support draft-and-publish"`.

### DS-08 — Unpublish Transitions to Unpublished with Last Revision
- **Status**: `Approved`
- **Origin**: `adr:004`
- **Given**: An existing published instance with revision `1` of a `DraftPublishOn` document type.
- **When**: Caller with `PublishDocument` permission invokes `service.unpublish(&caller, UnpublishDocumentCommand::new(instance_id, type_id)).await`.
- **Then**: Returns `Ok(unpublished)` with `publication_state` as `PublicationState::Unpublished { last_published_revision: 1, .. }`, audit version incremented, removes live published instance from wide table, and atomically archives the live revision leaving `[type]` into `document_snapshots` with `reason: SnapshotReason::Unpublish`.

### DS-09 — Unpublish Rejects Already Draft Instance
- **Status**: `Approved`
- **Origin**: `adr:004`
- **Given**: An existing draft instance of a `DraftPublishOn` document type.
- **When**: Authorized caller invokes `service.unpublish(&caller, UnpublishDocumentCommand::new(instance_id, type_id)).await`.
- **Then**: Returns `Err(ApplicationError::Domain(DomainError::InvalidStateTransition { .. }))`.

### DS-10 — Delete Removes Instance and Relations
- **Status**: `Approved`
- **Origin**: `adr:002`
- **Given**: An existing instance in repository.
- **When**: Authorized caller invokes `service.delete(&caller, DeleteDocumentCommand::new(instance_id, type_id)).await`.
- **Then**: Returns `Ok(())`, and subsequent `find_by_id` returns `Ok(None)`.

### DS-11 — Find Applies Filters and Returns Total Count
- **Status**: `Approved`
- **Origin**: `observed`
- **Given**: Three instances in repository, two matching `status == "active"` and one `status == "archived"`.
- **When**: Caller invokes `service.find(&caller, FindDocumentsCommand::new(type_id, pagination).with_filters(vec![filter("status", "active")])).await`.
- **Then**: Returns `Ok((items, total))` where `total == 2` and `items.len() == 2`.

### DS-12 — Unauthorized Caller Denied on All Mutations
- **Status**: `Approved`
- **Origin**: `adr:002`
- **Given**: A non-owner caller with no roles.
- **When**: Caller invokes `update`, `delete`, `publish`, or `unpublish`.
- **Then**: Returns `Err(ApplicationError::Unauthorized { .. })` without repository modification.

### DS-13 — Discard Draft Reverts to Published State
- **Status**: `Approved`
- **Origin**: `adr:004`
- **Given**: An existing document instance in `Modified` state (published with pending draft changes).
- **When**: Authorized caller invokes `service.discard_draft(&caller, DiscardDraftCommand::new(instance_id, type_id)).await`.
- **Then**: Returns `Ok(published)` with `publication_state` restored to clean `Published`, draft store cleared, and `has_pending_draft == false` without creating snapshots.

### DS-14 — Restore Snapshot Loads Historical Revision to Draft
- **Status**: `Approved`
- **Origin**: `adr:004` (updated decision 2026-10-22, Q1/Q2)
- **Given**: An existing published document instance with historical revision `1` in `document_snapshots` (superseded by revision `2` under archive-on-replace) and optional in-progress edits in `document_drafts`.
- **When**: Authorized caller invokes `service.restore_snapshot(&caller, cmd).await`:
  - If the document is in `Draft` state (never published): returns `Err(ApplicationError::Domain(DomainError::InvalidStateTransition { .. }))` (`LC-13`).
  - If the document is in `Modified` state with pending draft edits, and `cmd.discard_pending_edits` is `false`: returns `Err(ApplicationError::Conflict { code: ConflictCode::DraftHasPendingEdits, .. })` containing `"overwrite pending"`.
  - When confirmed with `cmd.discard_pending_edits == true` (or from clean `Published` state): restores content matching revision `1` into the working draft without creating checkpoints, marks draft status as `Modified` (or `Unpublished` if offline), and touches the audit trail; subsequent `FindByIdCommand` with `VersionView::Working` returns the restored draft (`revision 1` content), while default `VersionView::Published` continues returning the live published revision (`revision 2`).

### DS-15 — List Admin Headers Returns Lightweight Projections
- **Status**: `Approved`
- **Origin**: `adr:004`
- **Given**: Document instances in draft, modified, and published states in the repository.
- **When**: Authorized caller invokes `service.list_admin_headers(&caller, ListAdminHeadersCommand::new(type_id, pagination)).await`.
- **Then**: Returns `Ok((headers, total))` containing `DocumentHeader` items with `display_values`, `display_title`, and correct `DocumentStatus` badges without loading full content blobs.

### DS-16 — Unpublish Draft Resolution on Modified Document
- **Status**: `Approved`
- **Origin**: `adr:004`
- **Given**: An existing document instance in `Modified` state of a `DraftPublishOn` document type (holding both a live published revision and uncommitted working draft modifications).
- **When**: Authorized caller invokes `service.unpublish(&caller, cmd).await`:
  - With `UnpublishDraftResolution::KeepDraft` (default): keeps the working draft fields and relations in `VersionView::Working` while transitioning state to `PublicationState::Unpublished`, and archives the live published revision to `document_snapshots` (`SnapshotReason::Unpublish`).
  - With `UnpublishDraftResolution::ReplaceWithPublished`: replaces the working draft fields and relations with the live published revision content before transitioning state to `PublicationState::Unpublished`, and archives the live published revision to `document_snapshots` (`SnapshotReason::Unpublish`).
- **Then**: In both cases, the live published row is removed from the wide table (`VersionView::Published` returns `None`), and `VersionView::Working` returns the instance in `Unpublished` state.

### DS-17 — Publish Rejects Already Published Document Without Pending Changes
- **Status**: `Approved`
- **Origin**: `human` (decision 2026-10-22, see [`docs/content-lifecycle.md`](../../content-lifecycle.md))
- **Given**: An existing document of a `DraftPublishOn` type in clean `Published { revision: 1, .. }` state (no pending draft; `has_pending_draft == false`).
- **When**: Authorized caller invokes `service.publish(&caller, PublishDocumentCommand::new(instance_id, type_id)).await`.
- **Then**: Returns `Err(ApplicationError::Conflict { code: ConflictCode::AlreadyPublished, .. })` containing `"already published"`; the live revision stays `1`, the audit version is unchanged, and no snapshot is created.
- **Notes**: "Force re-publish" is intentionally not supported. Publishing is only meaningful from `Draft`, `Modified`, or `Unpublished`. The domain method `DocumentInstance::publish` still accepts `Published` because single-lifecycle types (`draft_and_publish: false`) use it for live updates (`DS-07`); the guard is a workflow policy of the draft-and-publish use case.

### DS-18 — Get Snapshot Returns Historical Revision
- **Status**: `Approved`
- **Origin**: `doc:docs/api.md`
- **Given**: An existing document instance with a historical revision snapshot recorded in `document_snapshots`.
- **When**: Authorized caller invokes `service.get_snapshot(&caller, GetSnapshotCommand::new(type_id, instance_id, revision)).await`.
- **Then**: Returns `Ok(snapshot)` containing the exact historical content and relations for `revision`, or returns `Err(ApplicationError::NotFound { entity: "DocumentSnapshot", .. })` if the revision does not exist.

---

## Open Questions

1. **Q1 (Subsequent SingleType Creation Response)**: SingleType duplicate creation currently returns `DomainError::SingleTypeAlreadyExists`. Should API translate this to HTTP 409 Conflict? (Currently handled as Domain error).
2. **Q2 (Filter Combination)**: In-memory filter logic uses logical AND across all filters. Is OR filtering needed in the future? (Out of scope for Milestone 1; AND is standard).

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `application/src/services/documents.rs:530` | `publish` returns `ApplicationError::Conflict` when `draft_and_publish` is false | Clear conflict condition | Consistent |
| 2 | `application/src/services/documents.rs:330` | SingleType check calls `exists_for_type` before validation | Fast-fail guard prevents unnecessary schema validation | Good optimization |
