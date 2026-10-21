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

`document_snapshots` is an append-only audit and revision history log. Every snapshot records the `reason` why that point-in-time state was captured:

| `reason` Value | Domain Variant (`SnapshotReason`) | Trigger / Workflow | Meaning |
|---|---|---|---|
| `'publish'` | `SnapshotReason::Publish` | `publish` (or auto-publish on `create`/`update`/`restore_snapshot` when `draft_and_publish: false`) | Recorded atomically inside `publish_transaction` whenever a document is published. Captures the exact fields and relations of the newly published `revision`. |
| `'checkpoint'` | `SnapshotReason::Checkpoint` | `DocumentSnapshotRepository::save_snapshot` (manual savepoint or pre-destructive backup) | An editorial or system savepoint created **without** publishing to the live wide table — e.g., when an editor saves a named milestone of a work-in-progress draft, or when the system automatically records a safety backup before a destructive action (such as overwriting draft edits during `restore_snapshot` or running a bulk schema migration). |
| `'unpublish'` | `SnapshotReason::Unpublish` | `unpublish` | Recorded atomically inside `unpublish_transaction` when a live document is taken offline. Captures an immutable audit record of the state (`revision = last_published_revision`) at the exact moment it was unpublished. |

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

### 3.3 Relational Junction Tables (`[type]_[attr]`, e.g. `article_tags`)
For each `HasMany` relation definition:
```sql
CREATE TABLE IF NOT EXISTS article_tags (
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    tag_id     UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (article_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_article_tags_tag_id ON article_tags (tag_id);
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
To avoid cross-shard distributed `FULL OUTER JOIN` operations in AWS Aurora DSQL, execute an indexed `UNION ALL`:
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
Executed inside an explicit `sqlx::Transaction`:

```rust
async fn publish_transaction(
    pool: &PgPool,
    instance: &DocumentInstance,
    revision: u32,
    display_values: &IndexMap<AttributeId, String>,
    display_title: &str,
    snapshot: &DocumentSnapshot,
) -> Result<(), RepositoryError> {
    let mut tx = pool.begin().await?;

    // 1. Upsert into wide table
    // (sets has_pending_draft = false, revision = revision, display_values, display_title, fields = instance.fields())
    sqlx::query(&upsert_wide_table_sql(&instance.document_type_id))
        .bind(...)
        .execute(&mut *tx)
        .await?;

    // 2. Synchronize junction rows
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

    // 3. Append immutable revision snapshot directly from snapshot
    sqlx::query(
        "INSERT INTO document_snapshots (id, document_type_id, document_id, revision, content, relations, created_at, created_by, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(snapshot.id.into_inner())
    .bind(snapshot.document_type_id.as_ref())
    .bind(snapshot.document_id.into_inner())
    .bind(snapshot.revision as i32)
    .bind(serde_json::to_value(&snapshot.fields)?)
    .bind(serde_json::to_value(&snapshot.relations)?)
    .bind(snapshot.created_at)
    .bind(snapshot.created_by.to_urn())
    .bind(snapshot.reason.as_str())
    .execute(&mut *tx)
    .await?;

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

### 4.4 Unpublish Transaction (`unpublish`)
```rust
async fn unpublish_transaction(
    pool: &PgPool,
    instance: &DocumentInstance,
    display_values: &IndexMap<AttributeId, String>,
    display_title: &str,
    snapshot: &DocumentSnapshot,
) -> Result<(), RepositoryError> {
    let mut tx = pool.begin().await?;

    // 1. Fetch current wide table row and ensure saved into document_drafts with status = 'unpublished'
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

    // 2. Delete from wide table [type] (cascading foreign keys remove junction rows)
    sqlx::query(&format!("DELETE FROM {} WHERE id = $1", instance.document_type_id.as_ref()))
        .bind(instance.id.into_inner())
        .execute(&mut *tx)
        .await?;

    // 3. Insert unpublish snapshot into document_snapshots with reason = 'unpublish'
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

    // 4. Commit transaction
    tx.commit().await?;
    Ok(())
}
```

---

### 4.5 Discard Draft (`discard_draft`)
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
