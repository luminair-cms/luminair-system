# Research: Audit Logging and Domain Events

- **Date**: 2026-10-05
- **Question**: How should Luminair capture, store, and expose audit trails and lifecycle events (such as document unpublishing, publication history, and role grants) across the domain, application, and infrastructure layers?
- **Related ADR**: [ADR-002](../adr/ADR-002-hexagonal-architecture.md)

---

## Background & Current State

In the current `domain` crate:
1. **`AuditTrail` struct** ([`domain/src/content/instance.rs`](../../domain/src/content/instance.rs)):
   - Tracks scalar metadata: `created_at`, `created_by`, `updated_at`, `updated_by`, and `version: u32`.
   - Updated in-place during edits via `DocumentInstance::touch(by, now)`.
2. **`PublicationState` enum**:
   - `Published { revision, published_at, published_by }` stores the publication actor and timestamp.
   - `Draft { last_published_revision: Option<u32> }` stores only the latest published revision number.
3. **The Unpublish Gap (Finding F2)**:
   - When `DocumentInstance::unpublish(now)` is called, the aggregate transitions from `Published` to `Draft`.
   - `published_at` and `published_by` are discarded from the current state.
   - The method signature currently accepts only `now: DateTime<Utc>`, not `by: Option<UserId>`.
   - While `audit.version` increments and `audit.updated_at` updates, no actor is recorded for the unpublish action, and no audit trail or historical record of the unpublish event persists within the domain model.

---

## Findings

### 1. Architectural Patterns for Audit and Lifecycle Tracking

#### Pattern A: Domain Events on Aggregate Roots (DDD Standard)
- **Mechanism**: The aggregate root collects domain events in memory during state transitions.
  ```rust
  pub struct DocumentInstance {
      // ... fields
      events: Vec<DomainEvent>,
  }

  pub enum DomainEvent {
      DocumentCreated { id: DocumentInstanceId, by: Option<UserId>, at: DateTime<Utc> },
      DocumentPublished { id: DocumentInstanceId, revision: u32, by: Option<UserId>, at: DateTime<Utc> },
      DocumentUnpublished { id: DocumentInstanceId, previous_revision: u32, by: Option<UserId>, at: DateTime<Utc> },
      DocumentTouched { id: DocumentInstanceId, version: u32, by: Option<UserId>, at: DateTime<Utc> },
  }
  ```
- **Lifecycle**:
  1. Aggregate mutates state and appends to `self.events`.
  2. Application use case calls repository to persist aggregate state.
  3. Application use case calls `instance.take_events()` and dispatches or stores them.
- **Characteristics**:
  - Pure domain: no I/O dependencies; events are plain data structs.
  - Highly expressive: captures the *intent* of the business action (e.g. `DocumentUnpublished`), not just raw field diffs.
  - Enables downstream side-effects (e.g. webhooks, search index invalidation, notifications).

#### Pattern B: Dedicated Append-Only Audit Log Port (Application Layer)
- **Mechanism**: An `AuditLogRepository` port defined in `domain` or `application`:
  ```rust
  pub trait AuditLogRepository: Send + Sync {
      async fn record(&self, entry: &AuditLogEntry) -> Result<(), RepositoryError>;
  }
  ```
- **Characteristics**:
  - Application use cases explicitly record audit rows alongside repository persistence.
  - Keeps aggregate roots lighter (no internal event buffers).
  - Can be captured via middleware or application service decorators.

#### Pattern C: Snapshot Revision History (Document Versioning)
- **Mechanism**: Every publication or major update writes an immutable revision snapshot row to a `document_revisions` table:
  - `revision_id`, `document_id`, `revision_number`, `snapshot_json`, `created_at`, `created_by`, `status` (`published`, `unpublished`).
- **Characteristics**:
  - Essential for CMS workflows requiring previewing or restoring past revisions ("rollback").
  - Complementary to audit logging: revision tables preserve full content snapshots, while audit logs preserve the log of who did what and when.

---

### 2. AWS Aurora DSQL & PostgreSQL Storage Considerations

1. **Transactional Insertion**:
   - DSQL supports transactional multi-table DML within a single transaction block.
   - An aggregate state update (`UPDATE document_instances`) and an audit event insertion (`INSERT INTO audit_logs`) can execute in the same transaction.
2. **Optimistic Concurrency Control (OCC)**:
   - Writing to an append-only table (`INSERT INTO audit_logs (id, ...)` with UUIDv7) does not cause row contention on previous log rows.
   - Contention only occurs if multiple transactions update the same `document_instances` row concurrently (status `40001` serialization failure).
3. **Transactional Outbox for Asynchronous Dispatch**:
   - If domain events trigger external AWS integrations (EventBridge, SQS, CloudWatch), dispatching them directly inside the HTTP request risks partial failures (DB commit succeeds, network call fails).
   - An outbox table (`INSERT INTO outbox_events`) written in the same DB transaction guarantees at-least-once delivery.

---

## Open Questions

1. **Unpublish Signature**: Should `DocumentInstance::unpublish` take `by: Option<UserId>, now: DateTime<Utc>` to update `audit.updated_by = by` even before a broader event/audit system is introduced?
2. **Scope of Audit Requirements**:
   - Does Luminair require an admin-facing audit trail in the UI (e.g., "View activity log for Article 123")?
   - Or is internal logging (structured tracing logs via `tracing::info!` to AWS CloudWatch) sufficient for v1?
3. **Content Revisions vs. Activity Logs**:
   - Should historical revisions (the actual document content at revision 1, revision 2, etc.) be modeled separately from general administrative activity logs (role grants, logins, schema changes)?

---

## Sources

- Martin Fowler, [Domain Event](https://martinfowler.com/eaaDev/DomainEvent.html)
- Vaughn Vernon, *Implementing Domain-Driven Design* (Chapter 8: Domain Events)
- AWS Aurora DSQL Documentation, [Transactions and Concurrency Control](https://docs.aws.amazon.com/aurora-dsql/latest/userguide/what-is-aurora-dsql.html)
- Enterprise Integration Patterns, [Transactional Outbox](https://microservices.io/patterns/data/transactional-outbox.html)
