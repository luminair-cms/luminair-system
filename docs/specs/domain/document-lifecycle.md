# LC — Document Instance Lifecycle Specification

- **Prefix**: `LC`
- **Layer**: `domain`
- **Unit under test**: `domain::content::DocumentInstance`
- **Format**: `T` (case table)
- **Test target**: `domain/tests/lc_document_lifecycle.rs`

---

## Vocabulary

| Term | Meaning / API symbol | Concrete representation |
|---|---|---|
| `Draft(None)` | Fresh draft never published | `PublicationState::Draft { last_published_revision: None }` |
| `Draft(Some(N))` | Unpublished draft with previous revision `N` | `PublicationState::Draft { last_published_revision: Some(N) }` |
| `Published(N)` | Published at revision `N` | `PublicationState::Published { revision: N, published_at: now, published_by: .. }` |
| `Publish(by, now)` | `instance.publish(by, now)` | Returns `Result<u32, DomainError>` |
| `Unpublish(by, now)` | `instance.unpublish(by, now)` | Returns `Result<(), DomainError>` |
| `Touch(by, now)` | `instance.touch(by, now)` | Updates `audit.updated_at`, `audit.updated_by`, increments `audit.version` |
| `Version(V)` | Audit trail version number | `instance.audit.version == V` |

---

## Rules

| ID | Status | Origin | Given State | Action / Input | Expected Result / Next State |
|---|---|---|---|---|---|
| `LC-01` | `Draft` | `adr:004` | `Draft(None)`, `Version(1)` | `Publish(Some("alice"), T1)` | `Ok(1)`, `Published(1)`, `Version(2)`, `updated_by == Some("alice")` |
| `LC-02` | `Draft` | `adr:004` | `Published(1)`, `Version(2)` | `Publish(Some("bob"), T2)` | `Ok(2)`, `Published(2)`, `Version(3)`, `updated_by == Some("bob")` |
| `LC-03` | `Draft` | `adr:004` | `Published(1)`, `Version(2)` | `Unpublish(Some("charlie"), T2)` | `Ok(())`, `Draft(Some(1))`, `Version(3)`, `updated_by == Some("charlie")` |
| `LC-04` | `Draft` | `adr:004` | `Draft(Some(1))`, `Version(3)` | `Publish(Some("alice"), T3)` | `Ok(2)`, `Published(2)`, `Version(4)`, `updated_by == Some("alice")` |
| `LC-05` | `Draft` | `adr:004` | `Draft(None)`, `Version(1)` | `Unpublish(Some("alice"), T1)` | `Err(DomainError::InvalidStateTransition)`, state unchanged, `Version(1)` |
| `LC-06` | `Draft` | `adr:004` | `Draft(Some(1))`, `Version(3)` | `Unpublish(Some("bob"), T4)` | `Err(DomainError::InvalidStateTransition)`, state unchanged, `Version(3)` |
| `LC-07` | `Draft` | `observed` | Initial `new(type, Some("alice"), T0)` | (constructed) | `Draft(None)`, `Version(1)`, `created_by == Some("alice")`, `updated_by == Some("alice")` |
| `LC-08` | `Draft` | `observed` | Any instance with `Version(V)` | `Touch(Some("bob"), T1)` | `audit.version == V + 1`, `audit.updated_by == Some("bob")`, `audit.updated_at == T1` |
| `LC-09` | `Draft` | `adr:004` | `Published(1)`, `Version(2)` | `EditDraft(Some("alice"), T3)` | `Modified(1)`, `Version(3)`, `updated_by == Some("alice")` |
| `LC-10` | `Draft` | `adr:004` | `Modified(1)`, `Version(3)` | `DiscardDraft()` | `Published(1)`, `Version(3)` |
| `LC-11` | `Draft` | `adr:004` | `Draft(None)`, `Version(1)` | `DiscardDraft()` | `Err(DomainError::InvalidStateTransition)` |
| `LC-12` | `Draft` | `adr:004` | `Published(2)`, `Version(4)` | `RestoreSnapshot(Rev1, T5)` | `Modified(1)`, `Version(5)` |

---

## Open Questions

1. **Q1 (Actor on Unpublish)**: When `unpublish` occurs, `audit.updated_by` is updated, but does `Draft` state need to remember *who* unpublished it, or is the audit trail alone sufficient? (Current behavior: audit trail alone records `updated_by`).
2. **Q2 (Version on Failed Action)**: A failed call to `unpublish` does not mutate audit trail or version. Confirmed as desired invariant.

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `domain/src/content/instance.rs:28-33` | Comment notes `content.fields` is public mutable, allowing edits without triggering `touch()` or version increments | DDD aggregate should encapsulate state changes | Acknowledged in ADR-001/decisions; postponed to persistence refactoring |
| 2 | `domain/src/content/instance.rs:38` | Aggregate holds `db_row_id: Option<DocumentInstanceId>` | Domain entities should not contain storage surrogate IDs | Postponed to persistence implementation |
