# Key Decisions Log

## 2026-10-02

### Regex Pattern Value Object in Schema
- **Decision**: Implemented `RegexPattern` value object wrapping `Arc<regex::Regex>` with `raw: String`.
- **Rationale**: `regex::Regex` does not implement `PartialEq`, `Eq`, or `Hash`. Because `FieldConstraint` is stored in a `HashSet` inside `FieldDefinition`, `FieldConstraint` requires `Eq` and `Hash`. `RegexPattern` solves this by delegating `PartialEq`, `Eq`, and `Hash` to the canonical raw regex string while keeping the precompiled regex available in an `Arc` for fast validation without runtime compilation.

### Aggregate Root Identifier Co-location
- **Decision**: Co-located entity identifiers directly within the aggregate root module (e.g. `DocumentTypeId` in `schema/documents.rs`, `AttributeId` in `schema/attributes.rs`, `DocumentInstanceId` in `content/instance.rs`, `RoleId` in `auth/role.rs`, `SystemConfigId` in `system/config.rs`) and dedicated standalone concepts (`UserId` in `auth/user.rs`, `LocaleId` in `system/locale.rs`). Removed all artificial `ids.rs` files.
- **Rationale**: Direct alignment with Clean DDD principles and Rust idioms. Identifiers are part of the aggregate root's boundary.

### Shared Test Support Module
- **Decision**: Created `domain::test_support` gated behind `#[cfg(any(test, feature = "test-support"))]` and exposed via `test-support = []` feature flag in `domain/Cargo.toml`.
- **Rationale**: Centralizes test data builders (`DocumentTypeBuilder`), ID generators, and canonical fixtures (`fixture_system_config`, `fixture_document_instance`, `fixture_access_request`) across unit tests in `domain` while allowing downstream crates (`application`, `infrastructure`) to reuse standard fixtures in their dev-dependencies without duplicating test setup logic.

## 2026-08-20 — Hexagonal Architecture & Crate Layout
- **Decision**: 3-crate Cargo workspace (`domain`, `application`, `infrastructure`) with unidirectional dependencies and pure domain.
- **Alternatives rejected**: Monolithic single-crate (poor boundary enforcement), traditional N-tier (leaks DB dependencies into domain).
- **See**: [`docs/adr/ADR-002-hexagonal-architecture.md`](../../docs/adr/ADR-002-hexagonal-architecture.md)

## 2026-08-21 — Database Choice: AWS Aurora DSQL with PostgreSQL Portability
- **Decision**: Aurora DSQL for cloud environments (serverless, scale-to-zero) with client UUIDv7, non-transactional DDL, and OCC retry; standard PostgreSQL for local/CI.
- **Alternatives rejected**: Aurora Postgres Serverless v2 (higher idle cost), DynamoDB (poor fit for relational document associations).
- **See**: [`docs/adr/ADR-003-aws-dsql-database.md`](../../docs/adr/ADR-003-aws-dsql-database.md)

## 2026-10-05 — Access Request Enrollment Invariant & Policy
- **Decision**: Added `DomainError::UserAlreadyEnrolled(UserId)` and pure domain policy `AccessRequestPolicy::validate_can_submit`. An access request is strictly an initial enrollment request for non-enrolled users. Users with existing role assignments or active requests are rejected at the policy boundary.
- **Rationale**: Eliminates the self-review paradox, decouples enrollment from general role changes, and enforces cross-aggregate invariants purely without I/O inside `domain`.

## 2026-10-05 — Two-Table Draft-and-Publish Pattern with Typed Relational Columns
- **Decision**: Adopt 2-table model per DocumentType (`[type]` main/draft table + `[type]_published` live snapshot table) with plain typed columns for B-tree search/filter/sort in PostgreSQL/DSQL. Unpublish deletes the live row, updates `[type].updated_by = user_id`, and appends an audit event.
- **Alternatives rejected**: Single JSONB column (poor sorting/filtering/constraints), single table with status/version columns (complex foreign keys, risk of public draft leak), dynamic view (cannot concurrently edit draft while live).
- **See**: [`docs/adr/ADR-004-draft-and-publish-table-pattern.md`](../../docs/adr/ADR-004-draft-and-publish-table-pattern.md)
