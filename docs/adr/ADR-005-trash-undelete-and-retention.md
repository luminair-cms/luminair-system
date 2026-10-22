# ADR-005: Trash, Undelete & Retention Architecture

- **Status**: Proposed
- **Date**: 2026-10-22 (proposed)
- **Deciders**: Dmitri Astafiev
- **Governing decisions**: [`ADR-002`](./ADR-002-hexagonal-architecture.md), [`ADR-003`](./ADR-003-aws-dsql-database.md), [`ADR-004`](./ADR-004-draft-and-publish-table-pattern.md)
- **Related docs**: [`docs/content-lifecycle.md`](../content-lifecycle.md), [`docs/design-infrastructure-persistence.md`](../design-infrastructure-persistence.md)

---

## Context

In content management systems, accidental document deletion is a catastrophic risk. Content authors frequently delete documents by mistake and expect a **"Trash / Recycle Bin"** with the ability to rollback (undelete / restore).

At the same time, deletion interacts directly with persistence in Aurora DSQL and PostgreSQL:
1. **Wide Table Cleanliness & Unique Constraints**: Each document type has its own wide table `[type]` with unique constraints (e.g. `slug VARCHAR(255) UNIQUE` or single-type singleton guarantees).
2. **Global Snapshot Store (`document_snapshots`)**: Snapshots archive immutable published revisions. When a document is deleted, what happens to its historical snapshots?
3. **Storage Growth & Vacuuming**: If snapshots and deleted records are retained indefinitely, storage grows unbounded. A predictable lifecycle is needed to purge old records (similar to Postgres `VACUUM` or AWS S3 lifecycle rules).
4. **Regulatory Erasure (GDPR / "Right to be Forgotten")**: Legal compliance requires permanent, unrecoverable removal of personal data upon request, overriding trash retention.

---

## Considered Alternatives

### Option 1: In-Place Soft Delete (`deleted_at` column in wide tables and drafts)

Add `deleted_at TIMESTAMPTZ` and `deleted_by VARCHAR(255)` columns to every wide table `[type]` and `document_drafts`.

- ❌ **Pollutes all visitor and admin queries**: Every query must remember `WHERE deleted_at IS NULL`. A missing filter accidentally exposes deleted content to the public.
- ❌ **Breaks unique constraints**: If `slug` has a unique constraint, deleting an article prevents reusing that slug unless partial unique indexes (`CREATE UNIQUE INDEX ... WHERE deleted_at IS NULL`) are used. Aurora DSQL does not support partial unique indexes.
- ❌ **Violates DDD boundaries**: Deleted entities linger in the active repository collection.

### Option 2: Dedicated `document_tombstones` Table + Snapshot Retention + Asynchronous Vacuum (Selected)

Separate active data from trash entirely:
- **Active tables (`[type]` and `document_drafts`)** contain **only live, active documents**. Deleting a document removes its row from active tables immediately, freeing slugs and singleton constraints.
- A dedicated static table `document_tombstones` holds the tombstoned instance state, pre-deletion status, payload, and retention expiration (`purge_after`).
- Historical snapshots in `document_snapshots` are preserved for audit and revision recovery while the tombstone exists.
- An asynchronous **Vacuum Task** permanently purges expired tombstones and their historical snapshots.
- An explicit **Hard Delete** command bypasses trash for GDPR permanent erasure.

- ✅ **Clean query model**: Visitor queries (`SELECT * FROM articles`) and draft queries never see deleted rows; no `WHERE deleted_at IS NULL` needed anywhere.
- ✅ **DSQL-friendly**: Compatible with standard unique constraints without partial index requirements.
- ✅ **Undelete capability**: Authors can restore deleted documents back to their previous state (`Draft`, `Published`, or `Unpublished`).
- ✅ **Storage safety**: Automated vacuum purges expired data according to retention policy.

### Option 3: Event-Sourced Archival

Record a `DocumentDeleted` event in an event log and rebuild instances on rollback.

- ❌ Excessive complexity for an asymmetric draft-and-publish architecture that uses stateful wide tables.

---

## Decision

We adopt **Option 2: Dedicated `document_tombstones` Table with Asymmetric Trash Lifecycle and Asynchronous Retention Vacuum**.

### 1. Persistence Schema (`document_tombstones`)

A single, global static table stores deleted document tombstones across all types:

```sql
CREATE TABLE IF NOT EXISTS document_tombstones (
    id                  UUID PRIMARY KEY,             -- Matches DocumentInstanceId
    document_type_id    VARCHAR(64) NOT NULL,
    display_title       VARCHAR(255) NOT NULL,
    status_at_deletion  VARCHAR(32) NOT NULL,         -- 'draft', 'published', 'modified', 'unpublished'
    payload             JSONB NOT NULL,               -- Full DocumentInstance state (fields, relations, audit)
    deleted_at          TIMESTAMPTZ NOT NULL,
    deleted_by          VARCHAR(255) NOT NULL,
    purge_after         TIMESTAMPTZ NOT NULL          -- deleted_at + retention_period (e.g. 30 days)
);

CREATE INDEX IF NOT EXISTS idx_document_tombstones_type_purge
    ON document_tombstones (document_type_id, purge_after);

CREATE INDEX IF NOT EXISTS idx_document_tombstones_purge
    ON document_tombstones (purge_after ASC);
```

### 2. Deletion Workflows

#### A. Soft Delete (`DocumentsService::delete`)
1. Checks `Permission::DeleteDocument(Some(type_id))`.
2. Loads the target instance (working copy if present, otherwise published).
3. In a single atomic database transaction:
   - Removes row from wide table `[type]` (if published or modified).
   - Removes row from `document_drafts` (if draft, modified, or unpublished).
   - Removes outbound relations from `document_links`.
   - Inserts row into `document_tombstones` with `purge_after = now + retention_period` (default: 30 days).
   - **Preserves** all rows in `document_snapshots` associated with `document_id`.

#### B. Undelete / Rollback (`DocumentsService::restore_deleted`)
1. Checks `Permission::DeleteDocument(Some(type_id))`.
2. Loads tombstone from `document_tombstones`.
3. Verifies that reviving the document does not violate unique constraints (e.g. slug collision with a newer document created while this was in trash):
   - If a collision occurs, returns `ApplicationError::Conflict("cannot restore document: unique constraint collision on field 'slug'")`.
4. In a single atomic database transaction:
   - Re-inserts or restores the document into its pre-deletion store (`document_drafts` or wide table `[type]`).
   - Restores relation links in `document_links`.
   - Deletes the row from `document_tombstones`.
5. Historical snapshots in `document_snapshots` remain intact and reconnect seamlessly to the restored document.

#### C. Asynchronous Retention Vacuum (`DocumentsService::vacuum_retention`)
1. Background cron/maintenance task (e.g. run daily).
2. Selects expired tombstones: `SELECT id FROM document_tombstones WHERE purge_after <= NOW() LIMIT $batch_size`.
3. For the batch of expired IDs:
   - In a transaction:
     - `DELETE FROM document_snapshots WHERE document_id = ANY($batch_ids);`
     - `DELETE FROM document_tombstones WHERE id = ANY($batch_ids);`

#### D. Hard Delete / GDPR Erasure (`DocumentsService::purge_permanently`)
1. Checks `Permission::ManageUsers` or `Permission::DeleteDocument` with admin role.
2. In a single atomic transaction:
   - Deletes from `[type]` wide table (if present).
   - Deletes from `document_drafts` (if present).
   - Deletes from `document_links` (if present).
   - Deletes from `document_tombstones` (if present).
   - Deletes from `document_snapshots` (all historical revisions).
3. Permanent, unrecoverable erasure complete.

---

## Consequences

### Positive
- **Zero Pollution of Live Queries**: Public wide tables contain only live rows. Queries are fast and free of soft-delete filters.
- **Aurora DSQL Compatibility**: No reliance on partial unique indexes; active unique constraints remain strict and functional.
- **Full Rollback Support**: Editors can inspect trash, preview deleted documents, and restore them with complete revision history.
- **Predictable Storage Bounds**: Expired tombstones and orphan snapshots are cleaned up automatically by the retention vacuum.
- **Regulatory Compliance**: Clean hard-delete path for GDPR right-to-be-forgotten requests.

### Negative / Trade-offs
- **Restore Slug Collision**: If a user deletes a document with slug `"news"` and creates a new document with slug `"news"`, restoring the deleted document will fail until the slug conflict is resolved.
- **Scheduled Maintenance Needed**: The application or infrastructure requires a scheduled background runner to execute `vacuum_retention`.
