# Infrastructure Persistence Architecture & Adapters

- **Target Module**: `infrastructure::persistence`
- **Governing Architecture**: [`ADR-003 (AWS DSQL Database)`](../adr/ADR-003-aws-dsql-database.md), [`ADR-004 (Asymmetric Hybrid Persistence)`](../adr/ADR-004-draft-and-publish-table-pattern.md), [`ADR-005 (Trash, Undelete & Retention)`](../adr/ADR-005-trash-undelete-and-retention.md)
- **Detailed Design Notes**: [`docs/design-infrastructure-persistence.md`](../design-infrastructure-persistence.md)
- **Status**: Active Architecture Specification
- **Last Updated**: 2026-10-23

---

## 1. Relational Persistence Overview

Luminair targets **AWS Aurora DSQL** (distributed, active-active PostgreSQL-compatible SQL engine) while maintaining 100% portability with standard **PostgreSQL** (for local development, Docker, and CI).

### Portability & Driver Contracts

| Component | Library / Driver | Role & Specifics |
|---|---|---|
| **SQL Engine & Pool** | `sqlx` (PostgreSQL driver) | Unified `sqlx::PgPool` for both PostgreSQL and Aurora DSQL |
| **Query Construction** | Parameterized SQL & `sea-query` | Dynamic query compilation for type-specific wide tables |
| **Serialization** | `serde_json` | JSONB serialization for `document_drafts` and `document_snapshots` |
| **Key Generation** | `uuid::Uuid::now_v7()` | Client-side time-ordered UUIDv7 (mandatory for DSQL; no serial/sequences) |
| **Authentication** | AWS SDK (`aws-sdk-dsql`) | Short-lived IAM token generation (15-min rotation) for Aurora DSQL |
| **Concurrency Control** | Infrastructure OCC Retry Loop | Automatic retry with exponential backoff on SQLSTATE `40001` (serialization failure) |

---

## 2. Naming Engine (`persistence::naming`)

The naming engine provides deterministic mapping from schema identifiers to SQL database identifiers conforming to PostgreSQL and Aurora DSQL length limits (max 63 bytes) and reserved keyword rules:

- **Document Wide Table**: `[type_id]` converted to snake_case and pluralized (e.g. `article` -> `articles`, `blog-post` -> `blog_posts`).
- **Junction Link Table**: `[source_table]_[rel_attr]_links` (e.g. `articles_tags_links`, `articles_category_links`).
- **Column Identifiers**: `[attr_id]` converted from kebab-case to snake-case (e.g. `hero-image-url` -> `hero_image_url`).
- **Standard Lifecycle Columns**:
  - `id UUID PRIMARY KEY`
  - `revision INT NOT NULL DEFAULT 1`
  - `created_at TIMESTAMPTZ NOT NULL`
  - `created_by VARCHAR(255) NOT NULL`
  - `updated_at TIMESTAMPTZ NOT NULL`
  - `updated_by VARCHAR(255) NOT NULL`
  - `published_at TIMESTAMPTZ NOT NULL`
  - `published_by VARCHAR(255) NOT NULL`
  - `has_pending_draft BOOLEAN NOT NULL DEFAULT FALSE`

---

## 3. Migration Architecture (`persistence::migration`)

Persistence migrations are divided into **static migrations** and **dynamic schema synchronization**:

### 3.1 Static System Migrations (`infrastructure/migrations/`)

Pre-compiled SQL migrations embedded via `sqlx::migrate!("./migrations")`:
1. `20260924000001_create_system_tables.sql`: `roles`, `role_permissions`, `user_role_assignments`, `access_requests`, `shadow_users`.
2. `20260924000002_seed_builtin_roles.sql`: seeds deterministic built-in roles (`admin`, `editor`, `viewer`, `writer`).
3. `20261010000001_create_document_drafts.sql`: unified draft table storing `status` (`'draft'`, `'modified'`, `'unpublished'`), `display_values JSONB`, `display_title`, `content JSONB`, `relations JSONB`, and OCC `version`.
4. `20261010000002_create_document_snapshots.sql`: append-only revision history with `reason IN ('publish', 'unpublish')` and `UNIQUE (document_type_id, document_id, revision)`.
5. `20261022000001_create_document_tombstones.sql`: trash/soft-delete store holding deleted instances pending retention vacuum.

All static migration files strictly begin with `-- no-transaction` for Aurora DSQL DDL compatibility and use client UUIDv7.

### 3.2 Dynamic Additive Schema Synchronizer (`persistence::migration::dynamic_sync`)

At application startup or during `RunMode::Migrate`:
1. Inspects active database tables and columns via `information_schema`.
2. Compares active DB schema against loaded `SchemaRegistry`.
3. Emits and executes non-destructive, additive DDL statements:
   - `CREATE TABLE IF NOT EXISTS [type] (...)`
   - `ALTER TABLE [type] ADD COLUMN IF NOT EXISTS [column] [sql_type]`
   - `CREATE TABLE IF NOT EXISTS [type]_[attr]_links (...)`
   - Indexes: `CREATE INDEX IF NOT EXISTS ...`

---

## 4. Query Engine & Concurrency (`persistence::query`)

Dynamic SQL is generated using `sea-query` to support type-specific columns, filtering, and pagination:

### 4.1 Wide Table Queries (`query::wide_table`)
- Parameterized inserts, updates, and selects against `[type]`.
- Field equality filtering (`FieldFilter`) and cursor/page pagination.
- Visitor query targeting only published rows (`WHERE has_pending_draft = FALSE` or directly on `[type]`).

### 4.2 Junction Relation Queries (`query::junction`)
- Inserts/deletes on junction tables (`source_id UUID`, `target_id UUID`, `order_index INT`).
- Two-phase batch relation loading for `fetch_relations`.

### 4.3 OCC Concurrency Retry Loop (`query::occ`)
Aurora DSQL enforces strict serializable isolation with optimistic concurrency control. Concurrent transactions targeting intersecting keys receive SQLSTATE `40001` (serialization failure).

`with_occ_retry` wraps transaction execution with randomized exponential backoff:
```rust
pub async fn with_occ_retry<T, F, Fut>(max_retries: usize, mut op: F) -> Result<T, PersistenceError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
```

---

## 5. Repository Adapters (`persistence::repositories`)

### 5.1 `SqlxDocumentInstanceRepository` & `SqlxDocumentSnapshotRepository`
Implements `domain::content::DocumentInstanceRepository` and `domain::content::DocumentSnapshotRepository` adhering to ADR-004:
- `save_draft`: writes working draft to `document_drafts`; updates `has_pending_draft = TRUE` in wide table if live row exists (`Modified` state).
- `publish`: writes to wide table `[type]`, clears draft row from `document_drafts` (`has_pending_draft = FALSE`), and archives superseded revision to `document_snapshots` if `snapshot: Option<&DocumentSnapshot>` is passed.
- `unpublish`: deletes row from wide table `[type]`, preserves state in `document_drafts` with `status = 'unpublished'`, and archives snapshot atomically.
- `discard_draft`: deletes pending draft from `document_drafts` and resets `has_pending_draft = FALSE` on the clean wide table row.
- `find_by_id(type, id, view)`: fetches from wide table (`VersionView::Published`) or `document_drafts` (`VersionView::Working`).
- `list_admin_headers`: single query combining drafts and clean published rows (`has_pending_draft = FALSE`) using materialized `display_values` and `display_title`.
- `find_display_titles_by_ids`: batched lookup for Phase 2 singular relational display field enrichment.

### 5.2 Auth Repositories
- `SqlxRoleRepository`: loads static and custom roles, manages granular permissions.
- `SqlxUserRoleAssignmentRepository`: maps user IDs to assigned roles with `ActorId` URNs.
- `SqlxAccessRequestRepository`: manages user onboarding queue with partial unique active index.
