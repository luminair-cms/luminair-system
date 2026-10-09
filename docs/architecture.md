# Architecture

## Overview

Luminair backend follows **Hexagonal Architecture** (Ports & Adapters) combined with **Domain-Driven Design (DDD)**.
The codebase is a Cargo workspace with three crates organised by layer:

```
luminair-service-aws-dsql/
├── Cargo.toml                 # workspace root
├── domain/                    # pure business logic — no I/O, no frameworks
├── application/               # use-cases / orchestration
└── infrastructure/            # adapters: HTTP, DB (AWS DSQL), external services
```

## Crate Responsibilities

### `domain`

- **Contains**: entities, value objects, aggregates, domain events, repository *traits* (ports), domain errors, structured into 5 DDD subdomains:
  - `auth`: Identity & Access Management
  - `schema`: Schema Metadata & Type System. Loaded in `infrastructure` from json and available as &'static
  - `content`: Content Lifecycle & Storage
  - `system`: System Configuration & Localization. Loaded in `infrastructure` from json and available as &'static
  - `common`: Ubiquitous Value Objects
- **Must NOT depend on**: `application`, `infrastructure`, any I/O crate (tokio, sqlx, axum …)
- **Allowed deps**: `serde` (for serialisation traits only), `thiserror`, `uuid`, `chrono`, `nutype` (for domain value objects), `email_address` and `url` (for validation predicates), `indexmap` (for preserving declared attribute order), `rust_decimal` (for exact decimal representation)


### `application`

- **Contains**: use-case structs, application services, command/query handlers (CQRS optional), application errors
- **Depends on**: `domain` only
- **Must NOT depend on**: `infrastructure`, `axum`, `sqlx`, or any adapter

### `infrastructure`

- **Contains**: HTTP handlers (axum), repository implementations (sqlx + AWS DSQL), schema migration, config loading, DI wiring
- **Depends on**: `domain`, `application`
- **This is the composition root** — the binary lives here (`infrastructure/src/main.rs`)

## Dependency Rule

```
infrastructure → application → domain
                             ↑
            (only domain traits cross this boundary)
```

No reverse dependencies. Domain traits (repository ports) are defined in `domain` and implemented in `infrastructure`.

## Key Patterns

| Pattern | Where used |
|---|---|
| Repository trait | `domain` (definition) / `infrastructure` (impl) |
| Command / Query | `application` — separate structs, no mixing |
| Domain Event | `domain` — raised by aggregates, handled in `application` |
| Error types | Each crate has its own; `infrastructure` maps all to HTTP status codes |
| Schema-driven | Content types defined via JSON schema; `domain` owns the schema entity |
| Asymmetric Hybrid Persistence | `domain` (ports) / `infrastructure` (impl) — see [`ADR-004`](./adr/ADR-004-draft-and-publish-table-pattern.md) |

## Content Persistence Pattern (ADR-004)

Luminair employs an **Asymmetric Hybrid Persistence Pattern** to balance public read performance with draft flexibility under AWS Aurora DSQL:

- **Per-Type Wide Relational Table (`[type]`)**: Contains only live published instances (and single-lifecycle types where `draft_and_publish: false`). Features typed columns, B-tree indexes, foreign keys, and `has_pending_draft BOOLEAN NOT NULL DEFAULT FALSE` to eliminate distributed cross-shard joins. Public visitor queries target this table directly.
- **Unified Draft Store (`document_drafts`)**: A single static system table holding in-progress content and relations as JSONB across all content types. Tracks `schema_version`, `status` (`draft`, `modified`, `unpublished`), and materialized `display_values` (`IndexMap<AttributeId, String>`) for rapid Admin table rendering.
- **Global Snapshot Store (`document_snapshots`)**: A single static append-only table recording immutable JSONB snapshots for publication history, manual checkpoints, and rollbacks with zero OCC write contention.
- **Relational Junction Tables (`[type]_[attr]`)**: Maintained strictly for published associations with foreign keys (`ON DELETE CASCADE`).
- **Composite Identity**: Schemas declare `display_fields: Vec<AttributeId>`, materialized at write-time into self-describing key-value maps (`display_values JSONB`) and a joined text fallback (`display_title`).

## Deployment Environments

The service is **designed for AWS** (Aurora DSQL, ECS / Lambda, IAM auth) but intentionally portable.
AWS-specific code is confined entirely to the `infrastructure` crate; the `domain` and `application`
crates have no knowledge of any deployment environment.

| Environment | Database | How to run |
|---|---|---|
| Local / dev | PostgreSQL (Docker) | `DATABASE_URL=postgres://...` in `.env` |
| CI | PostgreSQL | `#[sqlx::test]` auto-provisions |
| Docker / K8s (self-hosted) | PostgreSQL | Same binary; standard pg connection string |
| Staging / prod | AWS Aurora DSQL | IAM token auth; see [`docs/research/dsql-vs-postgres.md`](./research/dsql-vs-postgres.md) |

### Portability contract

- Repository traits are defined in `domain` — they are database-agnostic
- `infrastructure` provides a single unified set of repository implementations (`SqlxDocumentInstanceRepository`, `SqlxRoleRepository`, etc.) targeting `sqlx::PgPool`
- Database differences are isolated in the connection pool factory at startup:
  - Local / CI / K8s: Standard PostgreSQL connection pool using `DATABASE_URL`
  - AWS Aurora DSQL: Dynamic connection pool with IAM authentication token rotation (15-minute token lifecycle)
- Migrations use plain SQL and `sea-query`; avoid DSQL-incompatible DDL (see [`docs/research/dsql-vs-postgres.md`](./research/dsql-vs-postgres.md) and [`ADR-003`](./adr/ADR-003-aws-dsql-database.md)) so schemas run identically on standard PG and DSQL
- The binary is configured entirely through environment variables — no AWS SDK calls outside `infrastructure`

## UI Layer (Admin Dashboard)

The UI layer is an independent **Decoupled Single-Page Application (SPA)** adhering strictly to Hexagonal Architecture boundaries:
- **Location**: Isolated `/frontend` directory in the repository (independent package management, zero impact on Cargo workspace).
- **Technology Stack**: React 19, TypeScript, Mantine v7 design system (`@mantine/*`), Zustand for global client state, TanStack Router for type-safe routing, and TanStack Query v5 for server state.
- **Dynamic Schema Forms**: Queries `/api/schema/document-types` at runtime to generate forms for all 12 field types, multi-locale editing tabs (`LocalizedText`), and relational association pickers (`HasOne` and `HasMany`).
- **Authentication**: OIDC Authorization Code Flow with PKCE (`oidc-client-ts`). Uses Dex for local development (~15MB RAM via Docker Compose) and AWS Cognito in production.
- **Deployment**: Static assets hosted on AWS S3 behind an Amazon CloudFront CDN distribution with Origin Access Control (OAC). CloudFront routes `/*` to S3 and `/api/*` to the Axum backend, eliminating CORS preflight overhead in production.
- **Specification**: See [`docs/ui-architecture.md`](./ui-architecture.md).
