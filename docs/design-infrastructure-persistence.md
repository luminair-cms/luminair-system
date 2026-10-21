# Infrastructure Persistence Design Notes: Asymmetric Hybrid Pattern

- **Target Crate**: `infrastructure`
- **Governing Architecture**: [`ADR-004`](./adr/ADR-004-draft-and-publish-table-pattern.md), [`ADR-003`](./adr/ADR-003-aws-dsql-database.md)
- **Target Databases**: AWS Aurora DSQL (Primary), PostgreSQL (Local / CI / Docker)
- **Status**: Design Complete — Ready for Milestone 2 Implementation
- **Date**: 2026-10-09

---

## 1. Technology Stack & Driver Contracts

| Component | Library / Driver | Role & Specifics |
|---|---|---|
| **SQL Engine & Pool** | `sqlx` (PostgreSQL driver) | Unified `sqlx::PgPool` for both PostgreSQL and Aurora DSQL |
| **Query Construction** | Parameterized SQL & `sea-query` | Dynamic query compilation for type-specific wide tables |
| **Serialization** | `serde_json` | JSONB serialization for `document_drafts` and `document_snapshots` |
| **Key Generation** | `uuid::Uuid::now_v7()` | Client-side time-ordered UUIDv7 (mandatory for DSQL, no sequences) |
| **Authentication** | AWS SDK (`aws-sdk-dsql`) | Short-lived IAM token generation (15-min rotation) for Aurora DSQL |
| **Concurrency Control** | Infrastructure OCC Retry Loop | Automatic retry with exponential backoff on SQLSTATE `40001` |

---

## 2. Static System Migrations

Two static migration files must be placed in `infrastructure/migrations/`:

### Migration 1: `20261010000001_create_document_drafts.sql`
```sql
-- no-transaction

CREATE TABLE IF NOT EXISTS document_drafts (
    id                      UUID NOT NULL,
    document_type_id        VARCHAR(64) NOT NULL,
    display_values          JSONB NOT NULL DEFAULT '{}'::jsonb,
    display_title           VARCHAR(255) NOT NULL,
    status                  VARCHAR(32) NOT NULL CHECK (status IN ('draft', 'modified', 'unpublished')),
    content                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    relations               JSONB NOT NULL DEFAULT '{}'::jsonb,
    schema_version          INT NOT NULL DEFAULT 1,
    version                 INT NOT NULL DEFAULT 1,
    last_published_revision INT,
    created_at              TIMESTAMPTZ NOT NULL,
    created_by              VARCHAR(255) NOT NULL,
    updated_at              TIMESTAMPTZ NOT NULL,
    updated_by              VARCHAR(255) NOT NULL,
    PRIMARY KEY (document_type_id, id)
);

CREATE INDEX IF NOT EXISTS idx_document_drafts_type_updated
    ON document_drafts (document_type_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_document_drafts_author
    ON document_drafts (updated_by, updated_at DESC);
```

#### Semantics of `document_drafts.status` (`DocumentStatus`)

Note that `'published'` is **never** stored in `document_drafts` — a clean published document lives exclusively in its wide table `[type]` (where `has_pending_draft = FALSE`). Rows in `document_drafts` always have one of three statuses:

| `status` Value | Domain State (`PublicationState`) | Presence in Wide Table `[type]` | Meaning |
|---|---|---|---|
| `'draft'` | `PublicationState::Draft { last_published_revision: None }` | No row in `[type]` | A newly created document that has never been published (`last_published_revision IS NULL`). Exists only in `document_drafts`. |
| `'modified'` | `PublicationState::Modified { live_revision, draft_version, .. }` | Live row exists in `[type]` (`has_pending_draft = TRUE`) | A currently published document that has newer, unpublished working edits in `document_drafts`. Visitors still read the live revision from `[type]`, while editors work on the row in `document_drafts` (or revert it via `discard_draft`). |
| `'unpublished'` | `PublicationState::Unpublished { last_published_revision, .. }` | Removed from `[type]` | A document that was previously published (`last_published_revision IS NOT NULL`) and subsequently taken offline via `unpublish`. Its row is deleted from `[type]` so public queries cannot see it, while its full state is preserved in `document_drafts` for future editing or re-publishing. |

### Migration 2: `20261010000002_create_document_snapshots.sql`
```sql
-- no-transaction

CREATE TABLE IF NOT EXISTS document_snapshots (
    id               UUID PRIMARY KEY,
    document_type_id VARCHAR(64) NOT NULL,
    document_id      UUID NOT NULL,
    revision         INT NOT NULL,
    content          JSONB NOT NULL,
    relations        JSONB NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL,
    created_by       VARCHAR(255) NOT NULL,
    reason           VARCHAR(32) NOT NULL CHECK (reason IN ('publish', 'checkpoint', 'unpublish'))
);

CREATE INDEX IF NOT EXISTS idx_document_snapshots_lookup
    ON document_snapshots (document_type_id, document_id, revision DESC);
```

#### Semantics of `document_snapshots.reason` (`SnapshotReason`)

`document_snapshots` is an append-only archive of **historical revisions** that are no longer the active live row in `[type]` (or draft checkpoints). Every snapshot records the `reason` why that point-in-time state was captured:

| `reason` Value | Domain Variant (`SnapshotReason`) | Trigger / Workflow | Meaning |
|---|---|---|---|
| `'publish'` | `SnapshotReason::Publish` | `publish` (when overwriting an existing live row in `[type]`) | Recorded atomically inside `publish_transaction` when a **previous** live revision (`N - 1`) in `[type]` is superseded by a newly published revision (`N`). (On the first publish `revision = 1`, no previous row exists in `[type]`, so no `'publish'` snapshot is created yet). |
| `'checkpoint'` | `SnapshotReason::Checkpoint` | `restore_snapshot` (when in `Modified` state) or manual savepoint via `save_snapshot` | An editorial or system savepoint of a **working draft** created without publishing to the wide table — automatically recorded before `restore_snapshot` overwrites in-progress `Modified` draft changes, or triggered manually before risky edits/migrations. |
| `'unpublish'` | `SnapshotReason::Unpublish` | `unpublish` | Recorded atomically inside `unpublish_transaction` when a live document (`revision = N`) is taken offline from `[type]`. Archives the unpublished live version into `document_snapshots` before deleting the row from `[type]`. |

---

## 3. Dynamic Wide Table DDL Compiler

For each declared `DocumentType`, the schema migration service dynamically compiles and executes physical SQL DDL statements outside transaction blocks:

### 3.1 Field Type to SQL Mapping
* `PrimitiveType::Uid` $\rightarrow$ `VARCHAR(255) UNIQUE`
* `PrimitiveType::Uuid` $\rightarrow$ `UUID`
* `PrimitiveType::Text` $\rightarrow$ `TEXT`
* `PrimitiveType::Integer(I16)` $\rightarrow$ `SMALLINT`
* `PrimitiveType::Integer(I32)` $\rightarrow$ `INTEGER`
* `PrimitiveType::Integer(I64)` $\rightarrow$ `BIGINT`
* `PrimitiveType::Decimal { precision, scale }` $\rightarrow$ `NUMERIC(precision, scale)`
* `PrimitiveType::Date` $\rightarrow$ `DATE`
* `PrimitiveType::DateTime` $\rightarrow$ `TIMESTAMPTZ`
* `PrimitiveType::Boolean` $\rightarrow$ `BOOLEAN`
* `FieldType::Email` $\rightarrow$ `VARCHAR(320)`
* `FieldType::Url` $\rightarrow$ `TEXT`
* `FieldType::Json` $\rightarrow$ `JSONB`
* `FieldType::LocalizedText` $\rightarrow$ `JSONB`

### 3.2 Standard Wide Table Layout (`[type]`, e.g. `articles`)
```sql
CREATE TABLE IF NOT EXISTS articles (
    id                  UUID PRIMARY KEY,
    has_pending_draft   BOOLEAN NOT NULL DEFAULT FALSE,
    display_values      JSONB NOT NULL DEFAULT '{}'::jsonb,
    display_title       VARCHAR(255) NOT NULL,
    slug                VARCHAR(255) NOT NULL UNIQUE,
    title               TEXT NOT NULL,
    summary             TEXT,
    revision            INT NOT NULL DEFAULT 1,
    published_at        TIMESTAMPTZ NOT NULL,
    published_by        VARCHAR(255) NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL,
    created_by          VARCHAR(255) NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL,
    updated_by          VARCHAR(255) NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_articles_published_at ON articles (published_at DESC);
CREATE INDEX IF NOT EXISTS idx_articles_updated_at ON articles (updated_at DESC);
```

### 3.3 Relational Junction Tables (`[type]_[attr]_links`, e.g. `article_tags_links`)
For each `HasMany` relation definition:
```sql
CREATE TABLE IF NOT EXISTS article_tags_links (
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    tag_id     UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (article_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_article_tags_links_tag_id ON article_tags_links (tag_id);
```

---

## 4. Query Architecture & Adapter Implementations

### 4.1 Visitor Query (`find_published`)
Queries strictly the wide table `[type]`. Dynamically generates:
```sql
SELECT * FROM articles
WHERE published_at <= $1 AND category_id = $2
ORDER BY published_at DESC
LIMIT 25 OFFSET 0;
```
* **Performance**: Direct B-tree index lookup; zero joins with draft tables; zero status filtering.

### 4.1.2 Single Document Fetch by ID (`find_by_id`)

Single-document retrieval supports both public visitor queries and the Admin Studio's **2-tab editor** (`Published` and `Draft` tabs) through `find_by_id(type_id, id, view: VersionView)`:

- **Published (read-only) tab (`VersionView::Published`)**: Fetches strictly from the wide table via `find_by_id(type_id, id, VersionView::Published)`. This guarantees public visitors and editors inspecting live content always see the exact live representation without draft pollution.
- **Draft (editable) tab (`VersionView::Working`)**: Fetches the latest working state via `find_by_id(type_id, id, VersionView::Working)`. It checks `document_drafts` first; if absent, it falls back to the clean published row in the wide table.

#### State Resolution Across Editor Tabs

| Document State | In `document_drafts`? | In Wide Table `[type]`? | Published Tab (`VersionView::Published`) | Draft / Working Tab (`VersionView::Working`) | Description |
|---|---|---|---|---|---|
| **1. `'draft'`** *(never published)* | **Yes** (`status = 'draft'`) | No | `None` | `Some` (draft content) | Newly created document; no public version exists. |
| **2. `'published'`** *(clean live)* | No | **Yes** (`has_pending_draft = FALSE`) | `Some` (live content) | `Some` (live content via fallback) | Published live document with no uncommitted changes. Both tabs show identical content. |
| **3. `'modified'`** *(live + pending edits)* | **Yes** (`status = 'modified'`) | **Yes** (`has_pending_draft = TRUE`) | `Some` (live revision content) | `Some` (pending draft content) | Document is published live, but has uncommitted edits or a restored snapshot in the draft store. |
| **4. `'unpublished'`** *(taken offline)* | **Yes** (`status = 'unpublished'`) | No *(deleted on unpublish)* | `None` | `Some` (offline draft content) | Document was taken offline; public view returns 404/None, while draft tab preserves content and `last_published_revision`. |

#### SQL Implementations

**Path 1: `find_by_id` (`VersionView::Published`)**
Direct primary key index lookup against the wide table:
```sql
SELECT * FROM [type]
WHERE id = $1;
```
* **Performance**: Single-row point lookup by primary key; 0 joins; returns `None` if document is unpublished or still a draft.

**Path 2: `find_by_id` (`VersionView::Working`)**
Two-step lookup with wide-table fallback:
```sql
-- Step 1: Check unified draft store for working copy
SELECT * FROM document_drafts
WHERE document_type_id = $1 AND id = $2;

-- Step 2: If Step 1 returns NULL, fall back to clean published row
SELECT * FROM [type]
WHERE id = $1;
```
* **Correctness**: Preserves working draft isolation while seamlessly exposing clean published instances in the editable tab when no draft changes are pending.

---

### 4.2 Admin List Headers (`list_admin_headers`)

#### Path A: For Single-Lifecycle Types (`draft_and_publish: false`)
```sql
SELECT 
    id,
    display_values,
    display_title,
    'published' AS status,
    slug,
    revision,
    NULL::int AS draft_version,
    created_at, created_by,
    updated_at, updated_by,
    published_at, published_by
FROM [type]
ORDER BY updated_at DESC
LIMIT $1 OFFSET $2;
```

#### Path B: For Draft-Enabled Types (`draft_and_publish: true`)

Because a document can exist in `document_drafts`, in the wide table `[type]`, or in **both** simultaneously (when in `'modified'` state), here is where each document state physically lives and which branch of the `UNION ALL` selects it:

| Document State | In `document_drafts`? | In Wide Table `[type]`? | Selected by Branch | Why |
|---|---|---|---|---|
| **1. `'draft'`** *(never published)* | **Yes** (`status = 'draft'`) | No | **Branch 1** (`document_drafts`) | Only exists in `document_drafts`. |
| **2. `'unpublished'`** *(taken offline)* | **Yes** (`status = 'unpublished'`) | No *(deleted on unpublish)* | **Branch 1** (`document_drafts`) | Only exists in `document_drafts`. |
| **3. `'modified'`** *(live + pending edits)* | **Yes** (`status = 'modified'`) | **Yes** (`has_pending_draft = TRUE`) | **Branch 1** (`document_drafts`) | Exists in **both** tables. Branch 1 selects the working copy (with the latest edited `display_title`, `updated_at`, and `version`), while Branch 2 skips it via `WHERE has_pending_draft = FALSE` so the document is **not duplicated**. |
| **4. `'published'`** *(live, no pending edits)* | No *(deleted on publish / discard)* | **Yes** (`has_pending_draft = FALSE`) | **Branch 2** (`[type]`) | Only exists in `[type]` with `has_pending_draft = FALSE`. |

To avoid cross-shard distributed `FULL OUTER JOIN` operations in AWS Aurora DSQL, `list_admin_headers` executes this two-branch `UNION ALL`:
```sql
WITH combined AS (
    -- 1. Working copies in document_drafts ('draft', 'modified', 'unpublished')
    SELECT 
        id,
        display_values,
        display_title,
        status,
        NULL::text AS slug,
        last_published_revision AS revision,
        version AS draft_version,
        created_at, created_by,
        updated_at, updated_by,
        NULL::timestamptz AS published_at,
        NULL::text AS published_by
    FROM document_drafts
    WHERE document_type_id = $1

    UNION ALL

    -- 2. Clean published live items (where has_pending_draft = FALSE)
    SELECT 
        id,
        display_values,
        display_title,
        'published' AS status,
        slug,
        revision,
        NULL::int AS draft_version,
        created_at, created_by,
        updated_at, updated_by,
        published_at, published_by
    FROM [type]
    WHERE has_pending_draft = FALSE
)
SELECT * FROM combined
ORDER BY updated_at DESC
LIMIT $2 OFFSET $3;
```
* **DSQL Optimization**: Executes as two independent shard-friendly index scans merged in memory, scaling cleanly even with hundreds of thousands of documents.

#### Path C: Phase 2 Batched Display Title Enrichment (`find_display_titles_by_ids`)
When a `DocumentType`'s `display_fields` contains a singular relation (`HasOne` or `BelongsToOne`), `DocumentsService::list_admin_headers` collects the referenced target `DocumentInstanceId`s from the current page (~25 items) and executes one batched query per target type:
```sql
SELECT id, display_title
FROM [target_type]
WHERE has_pending_draft = FALSE AND id = ANY($1)
UNION ALL
SELECT id, display_title
FROM document_drafts
WHERE document_type_id = $2 AND id = ANY($1);
```
This resolves all related entity titles for the page in a single extra round-trip per target type without any write-time denormalization or event cascade complexity.

---

### 4.3 Atomic Publication Transaction (`publish`)
Executed inside an explicit `sqlx::Transaction`. Notice that `archived_snapshot: Option<&DocumentSnapshot>` is `Some` **only** when a previous live revision already existed in `[type]` (which is now being superseded by `revision`), and `None` on initial publication (`revision == 1`):

```rust
async fn publish_transaction(
    pool: &PgPool,
    instance: &DocumentInstance,
    revision: u32,
    display_values: &IndexMap<AttributeId, String>,
    display_title: &str,
    archived_snapshot: Option<&DocumentSnapshot>,
) -> Result<(), RepositoryError> {
    let mut tx = pool.begin().await?;

    // 1. If a previous live version existed in [type], archive it to document_snapshots (reason = 'publish')
    if let Some(snapshot) = archived_snapshot {
        sqlx::query(
            "INSERT INTO document_snapshots (id, document_type_id, document_id, revision, content, relations, created_at, created_by, reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'publish')"
        )
        .bind(snapshot.id.into_inner())
        .bind(snapshot.document_type_id.as_ref())
        .bind(snapshot.document_id.into_inner())
        .bind(snapshot.revision as i32)
        .bind(serde_json::to_value(&snapshot.fields)?)
        .bind(serde_json::to_value(&snapshot.relations)?)
        .bind(snapshot.created_at)
        .bind(snapshot.created_by.to_urn())
        .execute(&mut *tx)
        .await?;
    }

    // 2. Upsert the new live revision into wide table [type]
    // (sets has_pending_draft = false, revision = revision, display_values, display_title, fields = instance.fields())
    sqlx::query(&upsert_wide_table_sql(&instance.document_type_id))
        .bind(...)
        .execute(&mut *tx)
        .await?;

    // 3. Synchronize junction rows in [type]_[attr]_links
    sqlx::query(&delete_junction_sql(&instance.document_type_id))
        .bind(instance.id)
        .execute(&mut *tx)
        .await?;
    for target_id in &related_ids {
        sqlx::query(&insert_junction_sql(&instance.document_type_id))
            .bind(instance.id)
            .bind(target_id)
            .execute(&mut *tx)
            .await?;
    }

    // 4. Clean up draft store
    sqlx::query("DELETE FROM document_drafts WHERE document_type_id = $1 AND id = $2")
        .bind(instance.document_type_id.as_ref())
        .bind(instance.id.into_inner())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}
```

---

### 4.4 Unpublish Transaction (`unpublish` — Take Offline)

`unpublish` removes the live published state from `[type]` so the document is no longer visible to public visitors, and archives the unpublished live version into `document_snapshots` (`reason = 'unpublish'`).

When the document was in `'modified'` state (both a live row in `[type]` and pending edits in `document_drafts`), the caller selects `UnpublishDraftResolution`:
- **`UnpublishDraftResolution::KeepDraft` *(default)***: Preserves the in-progress draft edits in `document_drafts` and transitions its status to `'unpublished'`.
- **`UnpublishDraftResolution::ReplaceWithPublished`**: Replaces the draft in `document_drafts` with the live published content & relations (Optionally saving the overwritten draft as a `'checkpoint'` snapshot).

*(In both cases, `instance` passed into `unpublish_transaction` already holds the resolved target draft content/relations and `PublicationState::Unpublished`, while `snapshot` holds the live published revision being taken offline from `[type]`).*

```rust
async fn unpublish_transaction(
    pool: &PgPool,
    instance: &DocumentInstance,
    display_values: &IndexMap<AttributeId, String>,
    display_title: &str,
    snapshot: &DocumentSnapshot,
) -> Result<(), RepositoryError> {
    let mut tx = pool.begin().await?;

    // 1. Archive the live published version from [type] into document_snapshots with reason = 'unpublish'
    sqlx::query(
        "INSERT INTO document_snapshots (id, document_type_id, document_id, revision, content, relations, created_at, created_by, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'unpublish')"
    )
    .bind(snapshot.id.into_inner())
    .bind(snapshot.document_type_id.as_ref())
    .bind(snapshot.document_id.into_inner())
    .bind(snapshot.revision as i32)
    .bind(serde_json::to_value(&snapshot.fields)?)
    .bind(serde_json::to_value(&snapshot.relations)?)
    .bind(snapshot.created_at)
    .bind(snapshot.created_by.to_urn())
    .execute(&mut *tx)
    .await?;

    // 2. Delete from wide table [type] (cascading foreign keys remove [type]_[attr]_links rows)
    sqlx::query(&format!("DELETE FROM {} WHERE id = $1", instance.document_type_id.as_ref()))
        .bind(instance.id.into_inner())
        .execute(&mut *tx)
        .await?;

    // 3. Upsert resolved working copy into document_drafts with status = 'unpublished'
    sqlx::query(
        "INSERT INTO document_drafts (id, document_type_id, display_values, display_title, status, content, relations, schema_version, version, last_published_revision, created_at, created_by, updated_at, updated_by)
         VALUES ($1, $2, $3, $4, 'unpublished', $5, $6, 1, $7, $8, $9, $10, $11, $12)
         ON CONFLICT (document_type_id, id) DO UPDATE SET
             display_values = EXCLUDED.display_values,
             display_title = EXCLUDED.display_title,
             status = 'unpublished',
             content = EXCLUDED.content,
             relations = EXCLUDED.relations,
             version = EXCLUDED.version,
             last_published_revision = EXCLUDED.last_published_revision,
             updated_at = EXCLUDED.updated_at,
             updated_by = EXCLUDED.updated_by"
    )
    .bind(instance.id.into_inner())
    .bind(instance.document_type_id.as_ref())
    .bind(serde_json::to_value(display_values)?)
    .bind(display_title)
    .bind(serde_json::to_value(instance.fields())?)
    .bind(serde_json::to_value(&instance.relations)?)
    .bind(instance.audit.version as i32)
    .bind(snapshot.revision as i32)
    .bind(instance.audit.created_at)
    .bind(instance.audit.created_by.to_urn())
    .bind(instance.audit.updated_at)
    .bind(instance.audit.updated_by.to_urn())
    .execute(&mut *tx)
    .await?;

    // 4. Commit transaction
    tx.commit().await?;
    Ok(())
}
```

---

### 4.5 Discard Draft (`discard_draft`) & Restore Snapshot (`restore_snapshot`)

#### 4.5.1 Discard Draft (`discard_draft`)
```rust
async fn discard_draft_transaction(
    pool: &PgPool,
    type_id: &DocumentTypeId,
    id: DocumentInstanceId,
) -> Result<(), RepositoryError> {
    let mut tx = pool.begin().await?;

    // 1. Delete draft row
    sqlx::query("DELETE FROM document_drafts WHERE document_type_id = $1 AND id = $2")
        .bind(type_id.as_ref())
        .bind(id.into_inner())
        .execute(&mut *tx)
        .await?;

    // 2. Reset wide table pending flag
    sqlx::query(&format!("UPDATE {} SET has_pending_draft = FALSE WHERE id = $1", type_id.as_ref()))
        .bind(id.into_inner())
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}
```

#### 4.5.2 Restore Snapshot (`restore_snapshot` — Rollback to Historical Revision)
When rolling back to a previous revision from `document_snapshots`:
1. If the document is currently in `Modified` state (meaning an editor has uncommitted draft changes in `document_drafts`), `DocumentsService::restore_snapshot` first saves the current working draft to `document_snapshots` with **`reason = 'checkpoint'`** (`snapshot_repo.save_snapshot(&checkpoint)`), ensuring in-progress draft work is never lost.
2. The target historical snapshot's `fields` and `relations` are loaded into the working draft (`status = 'modified'` if a live version exists in `[type]`, or `'unpublished'` / `'draft'` otherwise) and persisted via `save_draft` (and `[type].has_pending_draft = TRUE`).
3. The editor can then review the restored content in the Draft tab and click **Publish** to make it live.

---

### 4.6 Multi-Repository Atomic Approval (`AccessRequest::approve`)
When an administrator approves an `AccessRequest`, both the newly generated `UserRoleAssignment` rows and the updated `AccessRequest` status (`Approved`) must be persisted atomically inside a single `sqlx::Transaction` wrapped in `with_occ_retry` so a partial failure cannot leave a user enrolled while their request remains `Pending`.

Attribution for reviews and grants uses `ActorId` URN serialization:
- `AccessRequest.reviewed_by: Option<ActorId>` is persisted via `.map(|a| a.to_urn())`.
- `UserRoleAssignment.granted_by: Option<ActorId>` is persisted via `.map(|a| a.to_urn())`.

---

## 5. AWS Aurora DSQL Operational Safeguards

### 5.1 Concurrency & OCC Retry Wrapper
Aurora DSQL utilizes Optimistic Concurrency Control. Concurrent write transactions can trigger SQL error code `40001` (`serialization_failure`).
All repository mutation methods wrap execution in an exponential backoff helper:

```rust
pub async fn with_occ_retry<F, Fut, T>(mut op: F) -> Result<T, RepositoryError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, RepositoryError>>,
{
    const MAX_RETRIES: u32 = 3;
    const BASE_DELAY_MS: u64 = 25;
    const MAX_DELAY_MS: u64 = 500;

    for attempt in 0..=MAX_RETRIES {
        match op().await {
            Ok(val) => return Ok(val),
            Err(RepositoryError::Sqlx(sqlx::Error::Database(ref db_err)))
                if db_err.code().as_deref() == Some("40001") && attempt < MAX_RETRIES =>
            {
                let delay = std::cmp::min(
                    MAX_DELAY_MS,
                    BASE_DELAY_MS * (2_u64.pow(attempt))
                );
                let jitter = rand::random::<u64>() % (delay / 2 + 1);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay + jitter)).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err(RepositoryError::OccConflictExhausted)
}
```
