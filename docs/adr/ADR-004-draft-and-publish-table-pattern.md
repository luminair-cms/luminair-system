# ADR-004: Two-Table Draft-and-Publish Pattern with Typed Relational Columns

- **Status**: Proposed
- **Date**: 2026-10-05
- **Deciders**: Dmitri Astafiev
- **Research**: [`docs/research/audit-logging-and-domain-events.md`](../research/audit-logging-and-domain-events.md), [`docs/research/dsql-vs-postgres.md`](../research/dsql-vs-postgres.md)

---

## Context

Luminair is a headless CMS requiring robust content authoring workflows. In standard editorial operations, authors must be able to edit in private while an existing published version of the document remains live for public visitors. Furthermore, editors require the ability to unpublish live content immediately when needed.

Content querying in a modern CMS demands real database filtering, searching, and sorting capabilities:
- Public visitors query by slug, filter by categories or tags, and sort by publication date (`WHERE status = 'published' AND category_id = ... ORDER BY published_at DESC`).
- Admin UI users search, filter, and sort across arbitrary schema attributes (e.g. titles, dates, author relations).

We need an architectural storage pattern in PostgreSQL and AWS Aurora DSQL that resolves three core requirements:
1. **Search, Filter, and Sort Performance**: Real relational columns vs. unstructured JSON/JSONB blobs.
2. **Draft Isolation & High-Performance Public Reads**: Allowing concurrent draft editing without public data leakage and without slow, error-prone query branching.
3. **Unpublish Semantics & Actor Attribution**: Clear tracking of who unpublished a document, when it occurred, and where the audit trail resides.

---

## Decision

Adopt the **Two-Table Pattern** per `DocumentType` with **typed relational columns**, complemented by dedicated junction tables for relations, audit trail attribution in the main draft table, and a planned post-MVP revision history snapshot table.

### 1. Plain SQL Columns for Schema Fields

Rather than storing all dynamic content attributes in a single opaque JSONB document, each `DocumentType` schema is mapped to physical, typed SQL columns (e.g., `title TEXT NOT NULL`, `slug VARCHAR(255) NOT NULL UNIQUE`, `summary TEXT`, `published_at TIMESTAMPTZ`):

- **B-tree Indexing**: Enables standard PostgreSQL and Aurora DSQL indexes on high-cardinality fields (`slug`, `published_at`, relation IDs).
- **Relational Integrity**: Enforces database-level constraints (foreign keys, nullability, uniqueness, check constraints).
- **Fast Sorting & Filtering**: Eliminates JSON path parsing overhead in SQL execution engines.

### 2. Two-Table Pattern per DocumentType

For each defined `DocumentType` (e.g. `articles`), two physical tables are maintained:

```
┌──────────────────────────────────────────────┐
│                  articles                    │  <-- Draft / Working Table (Admin UI)
├──────────────────────────────────────────────┤
│ id (UUIDv7, PK)                              │
│ title, slug, content, ... (typed columns)    │
│ status ('draft' | 'published')               │
│ revision (INT)                               │
│ created_at, created_by                       │
│ updated_at, updated_by                       │  <-- Captures author of latest edits / unpublish
│ published_at, published_by                   │  <-- Set on publish, cleared on unpublish
└──────────────────────┬───────────────────────┘
                       │
                       │ 1 : 0..1 (CASCADE)
                       ▼
┌──────────────────────────────────────────────┐
│              articles_published              │  <-- Public Live Snapshot (Public API)
├──────────────────────────────────────────────┤
│ id (UUIDv7, PK, FK -> articles(id))          │
│ title, slug, content, ... (typed columns)    │
│ revision (INT)                               │
│ published_at, published_by                   │
└──────────────────────────────────────────────┘
```

#### A. Main / Draft Table (`[type]`, e.g. `articles`)
- Acts as the primary identity holder and working draft for editorial staff.
- Holds full lifecycle audit metadata (`created_at`, `created_by`, `updated_at`, `updated_by`, `published_at`, `published_by`, `revision`).
- All admin edits mutate rows in this table without impacting public visitors.

#### B. Published Table (`[type]_published`, e.g. `articles_published`)
- Acts as an isolated, read-optimized snapshot of live published content.
- Primary key is `id UUID PRIMARY KEY REFERENCES [type](id) ON DELETE CASCADE`.
- Contains copies of all public-facing content columns at the moment of publication.
- **Public API reads query `[type]_published` directly**:
  - Zero joins required to filter out unpublished drafts.
  - Zero risk of leaking in-progress draft edits to public visitors.
  - Extremely fast index lookups (e.g. `SELECT * FROM articles_published WHERE slug = $1`).

#### C. Relational Junction Tables
- Many-to-many associations follow the same duality:
  - Working relations: `article_tags (article_id, tag_id)`
  - Published live relations: `article_tags_published (article_id, tag_id)` with foreign keys referencing `articles_published(id)`.
- On publication, junction records are synchronized within the same atomic database transaction.

### 3. Publication and Unpublish Lifecycle

#### Publication Workflow (`publish`)
When a document is published:
1. Validate draft content in `articles` against schema constraints.
2. In a single database transaction:
   - Update `articles`: set `status = 'published'`, `revision = revision + 1`, `published_at = $now`, `published_by = $user_id`, `updated_at = $now`, `updated_by = $user_id`.
   - Upsert into `articles_published`: copy content fields, `revision`, `published_at`, and `published_by`.
   - Synchronize relation junction records into `article_tags_published`.
   - Append audit log / domain event (`ArticlePublished`).

#### Unpublish Workflow (`unpublish`)
When an authorized user unpublishes a document:
1. **Immediate Public Removal**:
   - Execute `DELETE FROM articles_published WHERE id = $id`.
   - Cascading foreign keys immediately remove live junction rows (`article_tags_published`).
   - The document vanishes instantly from all public read queries.
2. **Attribution and Draft State Update in Main Table**:
   - The `user_id` of the actor performing the unpublish action is recorded directly in `articles.updated_by`:
     ```sql
     UPDATE articles
     SET status = 'draft',
         published_at = NULL,
         published_by = NULL,
         updated_at = $now,
         updated_by = $user_id
     WHERE id = $id;
     ```
   - Retains provenance: `articles.revision` and `last_published_revision` remain preserved so history is not lost.
3. **Audit Trail & Activity Logging**:
   - Record an append-only audit entry in the activity log (and emit `DocumentUnpublished` domain event):
     ```
     {
       action: "unpublish",
       document_type: "articles",
       document_id: $id,
       user_id: $user_id,
       at: $now,
       previous_revision: $revision
     }
     ```
   - This resolves why `user_id` is needed during `unpublish`: although the row in `articles_published` is removed, the actor is permanently recorded in `articles.updated_by` and in the audit log.

### 4. Post-MVP Revision History

In Post-MVP, full multi-version historical rollbacks will be introduced via a dedicated snapshot table:
- `document_revisions (id UUID PK, document_type VARCHAR, document_id UUID, revision_number INT, snapshot_json JSONB, created_at TIMESTAMPTZ, created_by UUID)`.
- Every publish action (and optional manual checkpoints) appends an immutable JSON snapshot.
- Rolling back restores content from `document_revisions` back into the main `articles` draft table for editing and subsequent republishing.
- This cleanly decouples high-performance runtime querying (2-table typed model) from cold historical archiving (JSON snapshot table).

---

## Considered Alternatives

### Alternative 1: Single Table with Single JSON/JSONB Column (`data JSONB`)
Store document attributes as an unstructured JSON/JSONB blob with a `status` column.
- *Pros*: Extreme flexibility for dynamic schemas; no DDL alterations when schemas change.
- *Cons*: Poor query performance for sorting and range filtering; complex, non-portable index syntax; no relational foreign key integrity; high CPU serialization overhead.

### Alternative 2: Single Table with Status Flag (`is_published`) and Draft Clones
Keep draft and published records in the same table, either via multiple rows per document or inline draft columns (`title_draft`, `title_published`).
- *Pros*: Single table per document type.
- *Cons*: 
  - Multiple rows per document break simple primary key lookups (`id` must be composite `(id, version)`), complicating foreign keys across junction tables.
  - Inline column doubling (`_draft`, `_published`) bloats table schemas and makes relational joins cumbersome.
  - Public queries must always remember to include `WHERE is_published = true`, creating serious risk of data leakage if a developer omits the filter.

### Alternative 3: Single Table with SQL View for Published Records
A single table with a status flag, accompanied by `CREATE VIEW articles_published AS SELECT * FROM articles WHERE status = 'published'`.
- *Pros*: Encapsulates filtering logic behind a view.
- *Cons*: Fails to solve the concurrent draft editing problem: any edit to a live document immediately mutates the live view, preventing authors from drafting changes privately while the published version remains untouched.

---

## Consequences

### Positive
- **Blazing Fast Public Reads**: Public visitor endpoints query `[type]_published` directly without joins or status checks.
- **True Draft Isolation**: Editorial teams can edit, refine, and save drafts privately without affecting public traffic.
- **Relational Power**: Native SQL sorting, filtering, B-tree indexes, and foreign keys operate natively on both PostgreSQL and AWS Aurora DSQL.
- **Clean Unpublish Semantics**: Simple row deletion from the published table, accompanied by clear audit attribution in `articles.updated_by` and append-only activity logs.
- **Forward Compatibility**: Clean bridge to post-MVP `document_revisions` snapshot table without rewriting the operational query architecture.

### Negative / Operational Constraints
- **Table Duplication**: Each document type requires two SQL tables (`[type]` and `[type]_published`) plus associated junction tables.
- **Atomic Publish Overhead**: The publish use-case must copy data from the draft table to the published table within a database transaction.
- **Schema Migrations**: Schema alterations (adding/modifying fields) must apply DDL to both draft and published tables.
