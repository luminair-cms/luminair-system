# ADR-004: Asymmetric Hybrid Draft-and-Publish Persistence Pattern

- **Status**: Accepted
- **Date**: 2026-10-05 (original) · 2026-10-09 (revised)
- **Deciders**: Dmitri Astafiev
- **Research**: [`docs/research/audit-logging-and-domain-events.md`](../research/audit-logging-and-domain-events.md), [`docs/research/dsql-vs-postgres.md`](../research/dsql-vs-postgres.md)

---

## Context

Luminair is a headless CMS requiring robust content authoring workflows. In standard editorial operations, authors must be able to edit in private while an existing published version of the document remains live for public visitors. Furthermore, editors require the ability to unpublish live content immediately, inspect revision history, and discard in-progress edits.

The original proposal for ADR-004 specified a **Symmetric Two-Table Pattern** where both the working draft table (`[type]`) and the published table (`[type]_published`) maintained identical wide, typed relational columns. However, architectural evaluation under AWS Aurora DSQL identified critical limitations with that approach:

1. **Non-Transactional DDL & Migration Overhead**: AWS Aurora DSQL does not support transactional DDL. Adding, altering, or removing fields across $N$ document types required executing DDL on $2N$ wide tables and $2M$ junction tables outside transactions, increasing deployment complexity.
2. **Draft Tolerance & Work-In-Progress Invariants**: Editors frequently save drafts with incomplete or missing fields. Enforcing SQL-level `NOT NULL` and relation constraints on a wide draft table either rejects partial drafts or forces all columns in the draft table to be `NULL`able, which dilutes relational integrity.
3. **Cross-Type Administrative Operations**: The CMS dashboard requires global views (e.g. "My Recent Drafts" across all document types). Having drafts scattered across dozens of individual type-specific tables makes cross-type aggregation cumbersome and inefficient.

We need an architectural storage pattern in PostgreSQL and AWS Aurora DSQL that resolves three core requirements:
1. **Search, Filter, and Sort Performance for Visitors**: Real relational columns and B-tree indexes for public queries without status checks or joins.
2. **Draft Ergonomics & Schema Agility**: Frictionless saving of work-in-progress content without relational constraint violations or dual-table DDL churn.
3. **Unified Revision Auditing**: Immutable, append-only historical snapshot tracking across all content types with zero Optimistic Concurrency Control (OCC) contention.

---

## Decision

Adopt the **Asymmetric Hybrid Persistence Pattern**:
1. **Per-Type Wide Relational Table (`[type]`)**: Dedicated strictly to **live published instances** (and single-lifecycle document types where `draft_and_publish: false`). Includes a `has_pending_draft` flag to eliminate distributed outer joins.
2. **Unified Draft Store (`document_drafts`)**: A **single static system table** holding working drafts as **JSONB** across all document types, augmented with materialized identity maps (`display_values JSONB` as key-value pairs) and workflow metadata.
3. **Global Snapshot Store (`document_snapshots`)**: A **single static system table** recording immutable **JSONB** snapshots for publication history, manual checkpoints, and rollbacks.
4. **Relational Junction Tables (`[type]_[attr]`)**: Maintained strictly for published associations with foreign keys (`ON DELETE CASCADE`).

---

## Detailed Storage Architecture

```
┌────────────────────────────────────────────────────────┐
│                   document_drafts                      │  <-- Single Static Table (JSONB)
├────────────────────────────────────────────────────────┤
│ (document_type_id, id) [PK]                            │
│ display_values (JSONB Object), display_title (VARCHAR) │
│ status ('draft' | 'modified' | 'unpublished')          │
│ content (JSONB), relations (JSONB)                     │
│ schema_version (INT), version (INT)                    │
│ last_published_revision (INT)                          │
│ created_at, created_by, updated_at, updated_by         │
└──────────────────────────┬─────────────────────────────┘
                           │
                           │ publish (validates & copies)
                           ▼
┌────────────────────────────────────────────────────────┐
│             [type] (e.g. articles)                     │  <-- Wide Relational Table (Visitors)
├────────────────────────────────────────────────────────┤
│ id (UUIDv7, PK)                                        │
│ has_pending_draft (BOOLEAN NOT NULL DEFAULT FALSE)     │  <-- Fast admin status flag
│ slug, title, body, ... (typed relational columns)      │
│ revision (INT), published_at, published_by             │
│ created_at, created_by, updated_at, updated_by         │
└──────────────────────────┬─────────────────────────────┘
                           │
                           │ 1 : N (FK ON DELETE CASCADE)
                           ▼
┌────────────────────────────────────────────────────────┐
│             [type]_[attr] (e.g. article_tags)          │  <-- Published Junction Table
├────────────────────────────────────────────────────────┤
│ (article_id, tag_id) [PK]                              │
└────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────┐
│                  document_snapshots                    │  <-- Global Immutable Revisions
├────────────────────────────────────────────────────────┤
│ id (UUIDv7, PK), document_type_id, document_id         │
│ revision (INT), content (JSONB), relations (JSONB)     │
│ created_at, created_by, reason ('publish'|'unpublish') │
└────────────────────────────────────────────────────────┘
```

### 1. Main Wide Table (`[type]`, e.g. `articles`)
- Contains only live published rows (or non-draft entities).
- Uses typed SQL columns matching the schema definition (`VARCHAR`, `INT`, `TIMESTAMPTZ`, etc.).
- B-tree indexed on high-traffic filter and sort keys (`slug`, `published_at`, relation IDs).
- Includes `has_pending_draft BOOLEAN NOT NULL DEFAULT FALSE`, updated when drafts are touched or cleared.
- Public visitor queries target this table directly: zero joins, zero status filtering, optimal execution plans.

### 2. Unified Draft Store (`document_drafts`)
- Single static table for all content types, created once during initial system migration.
- Primary key is composite `(document_type_id, id)`.
- Content attributes and relations are stored as JSONB. Foreign keys are soft references during drafting, avoiding locking issues or cascading deletions on drafts.
- Tracks `schema_version INT` to detect schema drift and allow graceful reconciliation.
- Materialized administrative columns for instant table rendering:
  - `display_values JSONB`: Key-value map (`{"attribute_id": "value"}`) resolved from `display_fields`.
  - `display_title VARCHAR(255)`: Flattened string for alphabetical sorting and autocomplete text search.
  - `status VARCHAR(32)`:
    - `'draft'`: Newly created document that has never been published (`last_published_revision IS NULL`; exists only in `document_drafts`).
    - `'modified'`: Document that is currently live in `[type]` (`has_pending_draft = TRUE`) and has pending, unpublished edits in `document_drafts`.
    - `'unpublished'`: Document that was previously published (`last_published_revision IS NOT NULL`) and later taken offline via `unpublish` (removed from `[type]`, preserved in `document_drafts`).
  - `version INT`: Incremented on every draft touch.

### 3. Composite Identity (`display_fields` & Key-Value `display_values`)
In `DocumentTypeInfo`, schemas define an ordered list of attributes representing the document's identity, subject to two strict schema-registration invariants:
- **Maximum 3 display fields total** (`display_fields.len() <= 3`).
- **Maximum 1 singular relational display field** (at most one attribute in `display_fields` may reference a `HasOne` or `BelongsToOne` relation; `HasMany` and `BelongsToMany` are forbidden).

```json
{
  "id": "pos-terminal",
  "info": {
    "title": "POS Terminal",
    "display_fields": ["terminal-id", "brand", "removing-date"]
  }
}
```
At write time, the domain engine extracts **scalar** and **localized scalar** values into:
- `display_values JSONB`: Self-describing object mapping `AttributeId -> String`:
  ```json
  {
    "terminal-id": "POS-042",
    "removing-date": "2026-05-12"
  }
  ```
  This allows frontend data tables to bind directly to `display_values["terminal-id"]` with custom typography (e.g. monospace font for IDs, badge for dates).
- `display_title VARCHAR(255)`: Concatenated fallback (`"POS-042 — 2026-05-12"`) indexed for alphabetical sorting (`ORDER BY display_title ASC`) and text search.
- **Singular Relations (`HasOne` / `BelongsToOne`) in `display_fields`**: Because `DocumentHeader` and composite display titles are used exclusively by the paginated Admin UI (~25 items per page), related entity titles are **not** materialized at write time (which would create stale titles whenever a related entity is renamed and require complex event-driven propagation). Instead, the single allowed relational display field is resolved via **Read-Time 2-Phase Batch Enrichment** inside `list_admin_headers`:
  1. Phase 1 loads the page of `DocumentHeader` rows (along with the singular relation's target ID if referenced in `display_fields`).
  2. Phase 2 executes **at most 1 batched lookup** (`WHERE id = ANY($1)`) against the target document type to fetch target `display_title`s and merges them into `display_values[rel_attr]` and `display_title` in memory.

### 4. Global Snapshot Store (`document_snapshots`)
- Single static table recording immutable revision history across all content types.
- Append-only inserts with UUIDv7 primary keys eliminate OCC serialization conflicts in DSQL.
- Stores frozen JSONB representation of content and relations along with `reason VARCHAR(32)` (`SnapshotReason`):
  - `'publish'` (`SnapshotReason::Publish`): Created atomically during `publish` (or auto-publish when `draft_and_publish: false`) to record the newly published revision.
  - `'checkpoint'` (`SnapshotReason::Checkpoint`): Manual or automatic editorial savepoint created without publishing to the wide table (e.g., saving a draft milestone or taking a pre-restore / pre-migration safety backup via `DocumentSnapshotRepository::save_snapshot`).
  - `'unpublish'` (`SnapshotReason::Unpublish`): Created atomically during `unpublish` to record an audit trail of the document state at the moment it was taken offline.

---

## Lifecycle Workflows

```
                     create()
                        │
                        ▼
                 ┌──────────────┐
                 │    DRAFT     │
                 │(drafts table)│
                 └──────┬───────┘
                        │
                        │ publish() [atomic transaction]
                        ▼
                 ┌──────────────┐
                 │  PUBLISHED   │
                 │ (wide table) │
                 └──────┬───────┘
                        │
             edit()     │       unpublish()
        ┌───────────────┴────────────────┐
        ▼                                ▼
┌────────────────┐               ┌──────────────┐
│    MODIFIED    │               │ UNPUBLISHED  │
│(in both tables)│               │(drafts table)│
└───────┬────────┘               └──────────────┘
        │
        ├── publish() ───────▶ updates wide table & snapshots; sets has_pending_draft = false
        └── discard_draft() ─▶ removes draft; sets has_pending_draft = false
```

### 1. Draft Creation (`create`)
- Validates syntax and field types (lenient draft validation).
- Inserts into `document_drafts` with `status = 'draft'`. The wide table is not touched.

### 2. Publication (`publish`)
- Verifies optimistic concurrency token (`expected_version` matches `draft.version`).
- Validates draft against strict schema constraints (required fields, regex patterns, number ranges, target existence).
- `DocumentInstanceRepository::publish` accepts `snapshot: &DocumentSnapshot` directly (along with `instance`, `revision`, `display_values`, and `display_title`) so the wide table, `document_drafts`, and `document_snapshots` are mutated within a single atomic database transaction:
  1. Upserts the live row into the wide table `[type]` and sets `has_pending_draft = FALSE`.
  2. Synchronizes relational junction rows in `[type]_[attr]`.
  3. Appends an immutable revision row to `document_snapshots` (`reason = 'publish'`).
  4. Deletes the draft row from `document_drafts`.
- If an OCC serialization conflict (`40001`) occurs in DSQL, the transaction runner retries automatically with exponential backoff and randomized jitter.

### 3. Editing a Published Document (`update`)
- The wide table continues serving public visitors without disruption.
- Mutates or creates a row in `document_drafts` with `status = 'modified'` and increments `version`.
- Updates `[type].has_pending_draft = TRUE` in the wide table.

### 4. Unpublishing (`unpublish`)
- `DocumentInstanceRepository::unpublish` accepts `snapshot: &DocumentSnapshot` directly (along with `instance`, `display_values`, and `display_title`) within a single atomic database transaction:
  1. Preserves content in `document_drafts` with `status = 'unpublished'`.
  2. Deletes the live row from the wide table `[type]` (foreign keys cascade to published junction rows).
  3. Appends an immutable revision row to `document_snapshots` (`reason = 'unpublish'`).

### 5. Discarding Drafts (`discard_draft`)
- Deletes the row from `document_drafts`.
- Updates `[type].has_pending_draft = FALSE`.
- The document remains cleanly published in the wide table.

### 6. Restoring Snapshots (`restore_snapshot`)
- Loads the historical snapshot from `document_snapshots`.
- Writes the snapshot content into `document_drafts` with `status = 'modified'` for editorial review and subsequent republishing.

---

## CQRS Query Separation

1. **Public Visitor Queries (`find_published`)**:
   - Queries strictly the wide table `[type]`.
   - Uses relational `FieldFilter`, B-tree indexing, and standard SQL sorting.
   - Zero draft leakage; optimal throughput.
2. **Admin Studio Listing (`list_admin_headers`)**:
   - For `draft_and_publish: false` types: queries the wide table directly (`status = 'published'`).
   - For `draft_and_publish: true` types: executes a union query between pure drafts (`document_drafts WHERE status = 'draft'`) and published documents from `[type]` (incorporating `has_pending_draft`), avoiding distributed cross-shard `FULL OUTER JOIN` bottlenecks.
   - If `display_fields` includes singular relations (`HasOne` / `BelongsToOne`), performs a 2nd-phase batched lookup (`find_display_titles_by_ids`) for the page's referenced target IDs and enriches `display_values` and `display_title` before returning.
   - Returns lightweight `DocumentHeader` projections (`display_values` key-value map, `display_title`, `status`, audit timestamps).
3. **Editing Fetch (`find_draft_by_id`)**:
   - Loads the full working draft JSONB by ID into the editor form (falling back to the wide table if no active draft exists).
   - Reconciles `draft.schema_version` against the current schema definition.
4. **Revision History (`list_snapshots`)**:
   - Scoped strictly to a single document ID (`WHERE document_type_id = $1 AND document_id = $2`).

---

## Considered Alternatives

### Alternative 1: Symmetric Dual Wide Tables (Original ADR-004)
Maintain identical wide tables `[type]` and `[type]_published`.
- *Rejected*: Requires dual DDL migrations for every schema update; rigid SQL constraints break incomplete draft saves; DSQL non-transactional DDL complexity.

### Alternative 2: Pure JSONB Single Table (`documents`)
Store all documents, published and drafts, in a single table with a `data JSONB` column.
- *Rejected*: Degrades public read performance, prevents B-tree range indexing, eliminates native SQL foreign key constraints, and increases CPU deserialization overhead on public traffic.

### Alternative 3: Positional Array for Display Values (`Vec<Option<String>>`)
Store display values as an anonymous array `["POS-042", "Nike Store", "2026-05-12"]`.
- *Rejected in favor of `IndexMap<AttributeId, String>`*: Positional arrays require client-side index alignment, break if schema order evolves, and make direct attribute lookups in UI tables brittle.

---

## Consequences

### Positive
- **Optimal Visitor Performance**: Public reads query pure typed relational columns with native B-tree indexes and zero draft filtering.
- **Minimal DDL Overhead**: Only published tables require DDL. `document_drafts` and `document_snapshots` are static tables created once.
- **True Draft Tolerance**: Authors can auto-save incomplete work without tripping SQL `NOT NULL` or foreign key constraints.
- **Rich Semantic UI Display**: Self-describing `display_values` key-value map enables dynamic multi-column tables, status chips, and custom attribute styling.
- **Resilient Distributed Querying**: `has_pending_draft` flag eliminates distributed cross-shard `FULL OUTER JOIN` operations in AWS Aurora DSQL.
- **Zero OCC Contention on Revisions**: Append-only snapshots with UUIDv7 eliminate write conflicts in Aurora DSQL.
- **Built-in Concurrency & Drift Defense**: `expected_version` guards against publish race conditions, and `schema_version` prevents stale draft corruption.

### Negative / Operational Constraints
- **Materialization Discipline**: Application services must consistently update `display_values` and `display_title` on every draft save and publish event.
- **Publish Transaction Scope**: Publishing requires orchestrating operations across the wide table, junction tables, draft store, and snapshot store within a single database transaction.
