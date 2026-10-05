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

