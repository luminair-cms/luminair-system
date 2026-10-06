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
| `SingleType` | Document type with `DocumentKind::SingleType` | Maximum 1 instance permitted |
| `Collection` | Document type with `DocumentKind::Collection` | Arbitrary number of instances |
| `DraftPublishOn` | Document type with `draft_and_publish: true` | Publish & unpublish enabled |
| `DraftPublishOff` | Document type with `draft_and_publish: false` | Publish & unpublish rejected |
| `OwnerCaller` | Caller whose `user_id` matches `instance.audit.created_by` | Allowed read, update, delete without role |
| `AdminCaller` | Caller possessing all system permissions | `CallerContext::system()` |
| `UnauthorizedCaller` | Caller without required permission or ownership | Triggers `ApplicationError::Unauthorized` |

---

## Rules

### DS-01 — Create Collection Draft Success
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: A valid collection document type in schema registry, and a caller with `CreateDocument` permission.
- **When**: Caller invokes `service.create(&caller, CreateDocumentCommand::new(type_id, valid_fields)).await`.
- **Then**: Returns `Ok(instance)` with `instance.audit.version == 1`, `instance.audit.created_by == Some(caller.user_id)`, `instance.content.publication_state` as `Draft { last_published_revision: None }`, and instance is retrievable from repository.

### DS-02 — Create Blocked for Second SingleType Instance
- **Status**: `Draft`
- **Origin**: `adr:002` (SingleType Guard Option C)
- **Given**: A single-type document type in schema registry, and an existing instance already saved in the repository.
- **When**: Any caller invokes `service.create(&caller, CreateDocumentCommand::new(type_id, valid_fields)).await`.
- **Then**: Returns `Err(ApplicationError::Domain(DomainError::SingleTypeAlreadyExists(type_id)))`.

### DS-03 — Create Rejects Content Violating Field Constraints
- **Status**: `Draft`
- **Origin**: `doc:domain::content::validate_content`
- **Given**: A document type with a required field `title` having `MinLength(5)`.
- **When**: Caller provides fields with `title` shorter than 5 characters or missing.
- **Then**: Returns `Err(ApplicationError::Validation(errors))` containing the failure reason; no instance is saved to the repository.

### DS-04 — Update Bumps Version and Touches Audit Trail
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: An existing document instance with `audit.version == 1`.
- **When**: Authorized caller updates a field via `service.update(&caller, UpdateDocumentCommand::new(type_id, instance_id, updated_fields)).await`.
- **Then**: Returns `Ok(updated)` with `updated.audit.version == 2`, `updated.audit.updated_by == Some(caller.user_id)`, and updated fields are persisted in the repository.

### DS-05 — Update Retains Unmentioned Fields and Clears Null Fields
- **Status**: `Draft`
- **Origin**: `observed` (PATCH semantics)
- **Given**: An instance holding optional fields `f1 = "val1"` and `f2 = "val2"`.
- **When**: Caller updates with `cmd.fields` containing only `f2: ContentValue::Null`.
- **Then**: `f1` retains `"val1"`, `f2` becomes `ContentValue::Null` (or removed), and other schema constraints pass.

### DS-06 — Publish Advances Revision on DraftPublish Enabled Type
- **Status**: `Draft`
- **Origin**: `adr:004`
- **Given**: An existing draft instance of a `DraftPublishOn` document type, and a caller with `PublishDocument` permission.
- **When**: Caller invokes `service.publish(&caller, PublishDocumentCommand::new(type_id, instance_id)).await`.
- **Then**: Returns `Ok(published)` with `publication_state` as `Published { revision: 1, .. }`, version incremented, and saved in repository.

### DS-07 — Publish Rejects Document of DraftPublish Disabled Type
- **Status**: `Draft`
- **Origin**: `adr:004`
- **Given**: An existing draft instance of a `DraftPublishOff` document type.
- **When**: Caller with `PublishDocument` permission invokes `service.publish(&caller, PublishDocumentCommand::new(type_id, instance_id)).await`.
- **Then**: Returns `Err(ApplicationError::Conflict(msg))` containing `"does not support draft-and-publish"`.

### DS-08 — Unpublish Transitions to Draft with Last Revision
- **Status**: `Draft`
- **Origin**: `adr:004`
- **Given**: An existing published instance with revision `1` of a `DraftPublishOn` document type.
- **When**: Caller with `PublishDocument` permission invokes `service.unpublish(&caller, UnpublishDocumentCommand::new(type_id, instance_id)).await`.
- **Then**: Returns `Ok(draft)` with `publication_state` as `Draft { last_published_revision: Some(1) }`, audit version incremented, and saved in repository.

### DS-09 — Unpublish Rejects Already Draft Instance
- **Status**: `Draft`
- **Origin**: `adr:004`
- **Given**: An existing draft instance of a `DraftPublishOn` document type.
- **When**: Authorized caller invokes `service.unpublish(&caller, UnpublishDocumentCommand::new(type_id, instance_id)).await`.
- **Then**: Returns `Err(ApplicationError::Domain(DomainError::InvalidStateTransition { .. }))`.

### DS-10 — Delete Removes Instance and Relations
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: An existing instance in repository.
- **When**: Authorized caller invokes `service.delete(&caller, DeleteDocumentCommand::new(type_id, instance_id)).await`.
- **Then**: Returns `Ok(())`, and subsequent `find_by_id` returns `Ok(None)`.

### DS-11 — Find Applies Filters and Returns Total Count
- **Status**: `Draft`
- **Origin**: `observed`
- **Given**: Three instances in repository, two matching `status == "active"` and one `status == "archived"`.
- **When**: Caller invokes `service.find(&caller, FindDocumentsCommand::new(type_id, pagination).with_filters(vec![filter("status", "active")])).await`.
- **Then**: Returns `Ok((items, total))` where `total == 2` and `items.len() == 2`.

### DS-12 — Unauthorized Caller Denied on All Mutations
- **Status**: `Draft`
- **Origin**: `adr:002`
- **Given**: A non-owner caller with no roles.
- **When**: Caller invokes `update`, `delete`, `publish`, or `unpublish`.
- **Then**: Returns `Err(ApplicationError::Unauthorized { .. })` without repository modification.

---

## Open Questions

1. **Q1 (Subsequent SingleType Creation Response)**: SingleType duplicate creation currently returns `DomainError::SingleTypeAlreadyExists`. Should API translate this to HTTP 409 Conflict? (Currently handled as Domain error).
2. **Q2 (Filter Combination)**: In-memory filter logic uses logical AND across all filters. Is OR filtering needed in the future? (Out of scope for Milestone 1; AND is standard).

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `application/src/services/documents.rs:421` | `publish` returns `ApplicationError::Conflict` when `draft_and_publish` is false | Clear conflict condition | Consistent |
| 2 | `application/src/services/documents.rs:278` | SingleType check calls `exists_for_type` before validation | Fast-fail guard prevents unnecessary schema validation | Good optimization |
