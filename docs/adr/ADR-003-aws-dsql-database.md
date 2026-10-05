# ADR-003: AWS Aurora DSQL with PostgreSQL Portability

- **Status**: Accepted
- **Date**: 2026-08-21
- **Deciders**: Dmitri Astafiev
- **Research**: [`docs/research/dsql-vs-postgres.md`](../research/dsql-vs-postgres.md)

---

## Context

Luminair targets a cloud-native, serverless deployment profile on AWS with minimal operational overhead (zero instance sizing, auto-scaling to zero, high availability across AZs). 

At the same time, local developer velocity and CI efficiency require fast, offline-capable test environments without mandatory AWS cloud dependencies or active Internet connections.

---

## Decision

Adopt **Amazon Aurora DSQL** as the production and staging relational database, while maintaining standard **PostgreSQL compatibility** for local development (Docker) and automated CI (`#[sqlx::test]`).

To guarantee seamless portability between standard PostgreSQL and Aurora DSQL without dialect fragmentation:

1. **Client-Generated UUIDv7 Primary Keys**:
   - Aurora DSQL does not support sequences or `SERIAL` / `BIGSERIAL`.
   - All primary keys are generated on the client side using UUIDv7 (`uuid::Uuid::now_v7()`), providing chronological index locality and zero round-trip ID generation.

2. **Standalone, Non-Transactional DDL**:
   - DDL statements (`CREATE TABLE`, `ALTER TABLE`) cannot run inside transaction blocks in DSQL.
   - All schema migrations must be written as standalone, non-transactional statements (`#[allow_non_transactional_migrations]`).

3. **Optimistic Concurrency Control (OCC) Handling**:
   - DSQL uses OCC rather than pessimistic row locks.
   - The infrastructure persistence layer must catch OCC serialization conflicts (`40001`) and retry queries with exponential backoff and jitter transparently before returning errors to callers.

4. **Dynamic IAM Token Authentication in Production**:
   - Staging/production connection pools use short-lived AWS IAM tokens (~15-minute rotation) managed dynamically by the infrastructure connection factory.
   - Local/CI environments connect via standard static PostgreSQL connection strings (`DATABASE_URL`).

---

## Considered Alternatives

1. **Standard Amazon Aurora PostgreSQL Serverless v2**:
   - *Pros*: 100% full PostgreSQL feature parity, transactional DDL.
   - *Cons*: Does not scale down to true zero; higher base baseline cost and operational configuration overhead.
2. **Amazon DynamoDB (NoSQL)**:
   - *Pros*: Proven serverless scale and low latency.
   - *Cons*: Weak fit for relational document associations, complex foreign key constraints, dynamic content queries, and filtering.

---

## Consequences

### Positive
- Fully managed serverless scalability with pay-per-request pricing and zero capacity planning.
- Developers can develop and test entirely offline against local PostgreSQL Docker containers or in-memory CI databases.
- Single unified SQL repository codebase (`sqlx::PgPool`) serves both environments.

### Negative / Operational Constraints
- Application code cannot rely on Postgres-specific extensions, sequences, or `LISTEN`/`NOTIFY`.
- Write contention under high concurrency requires robust OCC retry strategies in repository adapters.
