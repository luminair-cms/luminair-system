# LC — Document Instance Lifecycle Specification

- **Prefix**: `LC`
- **Layer**: `domain`
- **Unit under test**: `domain::content::DocumentInstance`
- **Format**: `T` (case table)
- **Test target**: `domain/tests/lc_document_lifecycle.rs`
- **Explainer**: [`docs/content-lifecycle.md`](../../content-lifecycle.md) (diagrams and full state × action matrix, including application-level guards)

> [!NOTE]
> These rules cover the **aggregate** only. `LC-02` (`Published → publish`) is valid at the domain level because single-lifecycle types (`draft_and_publish: false`) advance the live revision on every save. For draft-and-publish types, the application rejects it (`DS-17`).

---

## Vocabulary

| Term | Meaning / API symbol | Concrete representation |
|---|---|---|
| `Draft` | Fresh draft never published | `PublicationState::Draft` |
| `Published(N)` | Published at revision `N` | `PublicationState::Published { revision: N, published_at, published_by }` |
| `Modified(N, V)` | Published at revision `N` with pending draft version `V` | `PublicationState::Modified { live_revision: N, published_at, published_by, draft_version: V }` |
| `Unpublished(N)` | Unpublished content with historical revision `N` | `PublicationState::Unpublished { last_published_revision: N, unpublished_at, unpublished_by }` |
| `Publish(by, now)` | `instance.publish(by, now)` | Returns `Result<u32, DomainError>` |
| `Unpublish(by, now)` | `instance.unpublish(by, now)` | Returns `Result<(), DomainError>` |
| `edit_draft` | `instance.edit_draft(actor, now)` | Transitions to `Modified` (or bumps draft version) |
| `discard_draft` | `instance.discard_draft()` | Restores clean `Published` state, returns `Result<(), DomainError>` |
| `restore_snapshot` | `instance.restore_snapshot(&snapshot, actor, now)` | Restores historical snapshot into draft, returns `Result<(), DomainError>` |
| `reconstitute` | `DocumentInstance::reconstitute(id, type_id, content, relations, audit)` | Direct reconstitution from persistence without triggering domain events |
| `Touch(by, now)` | `instance.touch(by, now)` | Updates `audit.updated_at`, `audit.updated_by`, increments `audit.version` |
| `Version(V)` | Audit trail version number | `instance.audit.version == V` |

---

## Rules

| ID | Status | Origin | Given State | Action / Input | Expected Result / Next State |
|---|---|---|---|---|---|
| `LC-01` | `Approved` | `adr:004` | `Draft`, `Version(1)` | `Publish(Actor("alice"), T1)` | `Ok(1)`, `Published(1)`, `Version(2)`, `updated_by == Actor("alice")` |
| `LC-02` | `Approved` | `adr:004` | `Published(1)`, `Version(2)` | `Publish(Actor("bob"), T2)` | `Ok(2)`, `Published(2)`, `Version(3)`, `updated_by == Actor("bob")` |
| `LC-03` | `Approved` | `adr:004` | `Published(1)`, `Version(2)` | `Unpublish(Actor("charlie"), T2)` | `Ok(())`, `Unpublished(1)`, `Version(3)`, `updated_by == Actor("charlie")` |
| `LC-04` | `Approved` | `adr:004` | `Unpublished(1)`, `Version(3)` | `Publish(Actor("alice"), T3)` | `Ok(2)`, `Published(2)`, `Version(4)`, `updated_by == Actor("alice")` |
| `LC-05` | `Approved` | `adr:004` | `Draft`, `Version(1)` | `Unpublish(Actor("alice"), T1)` | `Err(DomainError::InvalidStateTransition)`, state unchanged, `Version(1)` |
| `LC-06` | `Approved` | `adr:004` | `Unpublished(1)`, `Version(3)` | `Unpublish(Actor("bob"), T4)` | `Err(DomainError::InvalidStateTransition)`, state unchanged, `Version(3)` |
| `LC-07` | `Approved` | `adr:004` | (none) | `new(type, Actor("alice"), T0)` | `Draft`, `Version(1)`, `created_by == Actor("alice")`, `updated_by == Actor("alice")` |
| `LC-08` | `Approved` | `adr:004` | `observed` | Any instance with `Version(V)` | `Touch(Actor("bob"), T1)` | `audit.version == V + 1`, `audit.updated_by == Actor("bob")`, `audit.updated_at == T1` |
| `LC-09` | `Approved` | `adr:004` | `Published(1)`, `Version(2)` | `EditDraft(Actor("alice"), T3)` | `Modified(1)`, `Version(3)`, `updated_by == Actor("alice")` |
| `LC-10` | `Approved` | `adr:004` | `Modified(1)`, `Version(3)` | `DiscardDraft()` | `Published(1)`, `Version(3)` |
| `LC-11` | `Approved` | `adr:004` | `Draft`, `Version(1)` | `DiscardDraft()` | `Err(DomainError::InvalidStateTransition)` |
| `LC-12` | `Approved` | `adr:004` | `Published(2)`, `Version(4)` | `RestoreSnapshot(Rev1, T5)` | `Modified(2)`, `Version(5)` |
| `LC-13` | `Approved` | `human` | `Draft`, `Version(1)` | `RestoreSnapshot(Rev1, T1)` | `Err(DomainError::InvalidStateTransition)` |

---

## Open Questions

1. **Q1 (Actor on Unpublish)**: [Resolved in ADR-004] `PublicationState::Unpublished` explicitly records `unpublished_by: ActorId` and `unpublished_at: DateTime<Utc>` in addition to updating `audit.updated_by` and `audit.updated_at`.
2. **Q2 (Version on Failed Action)**: A failed call to `unpublish` does not mutate audit trail or version. Confirmed as desired invariant.

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `domain/src/content/instance.rs` | `content.fields` is private and encapsulated behind `fields()`, `get_field()`, `set_field()`, `remove_field()`, and `replace_fields()` | DDD aggregate should encapsulate state changes | **Resolved** in Step 1.3 |
| 2 | `domain/src/content/instance.rs` | Aggregate contains clean domain fields only | Leaked storage surrogate `db_row_id` removed | **Resolved** in Step 1.3 |
