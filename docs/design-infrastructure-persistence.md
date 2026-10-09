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
    created_by              VARCHAR(255),
    updated_at              TIMESTAMPTZ NOT NULL,
    updated_by              VARCHAR(255),
    PRIMARY KEY (document_type_id, id)
);

CREATE INDEX IF NOT EXISTS idx_document_drafts_type_updated
    ON document_drafts (document_type_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_document_drafts_author
    ON document_drafts (updated_by, updated_at DESC);
```

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
    created_by       VARCHAR(255),
    reason           VARCHAR(32) NOT NULL CHECK (reason IN ('publish', 'checkpoint', 'unpublish'))
);

CREATE INDEX IF NOT EXISTS idx_document_snapshots_lookup
    ON document_snapshots (document_type_id, document_id, revision DESC);
```

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
    published_by        VARCHAR(255),
    created_at          TIMESTAMPTZ NOT NULL,
    created_by          VARCHAR(255),
    updated_at          TIMESTAMPTZ NOT NULL,
    updated_by          VARCHAR(255)
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
    -- 1. Unpublished pure drafts
    SELECT 
        id,
        display_values,
        display_title,
        'draft' AS status,
        NULL::text AS slug,
        NULL::int AS revision,
        version AS draft_version,
        created_at, created_by,
        updated_at, updated_by,
        NULL::timestamptz AS published_at,
        NULL::text AS published_by
    FROM document_drafts
    WHERE document_type_id = $1 AND status = 'draft'

    UNION ALL

    -- 2. Published & Modified live items (driven by has_pending_draft)
    SELECT 
        id,
        display_values,
        display_title,
        CASE WHEN has_pending_draft THEN 'modified' ELSE 'published' END AS status,
        slug,
        revision,
        NULL::int AS draft_version,
        created_at, created_by,
        updated_at, updated_by,
        published_at, published_by
    FROM [type]
)
SELECT * FROM combined
ORDER BY updated_at DESC
LIMIT $2 OFFSET $3;
```
* **DSQL Optimization**: Executes as two independent shard-friendly index scans merged in memory, scaling cleanly even with hundreds of thousands of documents.

---

### 4.3 Atomic Publication Transaction (`publish`)
Executed inside an explicit `sqlx::Transaction`:

```rust
async fn publish_transaction(
    pool: &PgPool,
    type_id: &'static DocumentTypeId,
    instance: &DocumentInstance,
    revision: u32,
    actor: Option<&UserId>,
    now: DateTime<Utc>,
) -> Result<(), RepositoryError> {
    let mut tx = pool.begin().await?;

    // 1. Upsert into wide table
    // (sets has_pending_draft = false, revision = revision, published_at = now)
    sqlx::query(&upsert_wide_table_sql(type_id))
        .bind(...)
        .execute(&mut *tx)
        .await?;

    // 2. Synchronize junction rows
    sqlx::query(&delete_junction_sql(type_id))
        .bind(instance.id)
        .execute(&mut *tx)
        .await?;
    for target_id in &related_ids {
        sqlx::query(&insert_junction_sql(type_id))
            .bind(instance.id)
            .bind(target_id)
            .execute(&mut *tx)
            .await?;
    }

    // 3. Append immutable revision snapshot
    sqlx::query(
        "INSERT INTO document_snapshots (id, document_type_id, document_id, revision, content, relations, created_at, created_by, reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'publish')"
    )
    .bind(Uuid::now_v7())
    .bind(type_id.as_ref())
    .bind(instance.id.into_inner())
    .bind(revision as i32)
    .bind(serde_json::to_value(&instance.content.fields)?)
    .bind(serde_json::to_value(&instance.relations)?)
    .bind(now)
    .bind(actor.map(|u| u.as_ref()))
    .execute(&mut *tx)
    .await?;

    // 4. Clean up draft store
    sqlx::query("DELETE FROM document_drafts WHERE document_type_id = $1 AND id = $2")
        .bind(type_id.as_ref())
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
    type_id: &'static DocumentTypeId,
    id: DocumentInstanceId,
    actor: Option<&UserId>,
    now: DateTime<Utc>,
) -> Result<(), RepositoryError> {
    let mut tx = pool.begin().await?;

    // 1. Fetch current wide table row and ensure saved into document_drafts with status = 'unpublished'
    // 2. Delete from wide table [type] (cascading foreign keys remove junction rows)
    sqlx::query(&format!("DELETE FROM {} WHERE id = $1", type_id.as_ref()))
        .bind(id.into_inner())
        .execute(&mut *tx)
        .await?;

    // 3. Insert unpublish snapshot into document_snapshots with reason = 'unpublish'
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
    type_id: &'static DocumentTypeId,
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
