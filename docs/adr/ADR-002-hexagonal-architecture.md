# ADR-002: Hexagonal Architecture and DDD Crate Layout

- **Status**: Accepted
- **Date**: 2026-08-20
- **Deciders**: Dmitri Astafiev
- **Research**: [`docs/architecture.md`](../architecture.md)

---

## Context

Luminair is a headless CMS and content platform requiring clean boundaries between business rules, orchestration workflows, and external technology adapters (HTTP APIs, persistence engines, auth providers). 

Without strict isolation, domain logic tends to leak into database query handlers or HTTP route handlers, complicating testing, schema migrations, and future service refactoring.

---

## Decision

Adopt **Hexagonal Architecture (Ports and Adapters)** combined with **Domain-Driven Design (DDD)**, structured into a 3-crate Cargo workspace with strict unidirectional compile-time boundaries:

1. **`domain` (Core Business Logic)**:
   - Houses entities, aggregates, value objects, domain events, domain errors, and repository traits (**ports**).
   - Structured into 5 subdomains: `auth`, `schema`, `content`, `system`, and `common`.
   - **Zero I/O**: strictly no dependencies on async runtimes (`tokio`), web frameworks (`axum`), database drivers (`sqlx`), or external network crates.

2. **`application` (Use Case Orchestration)**:
   - Contains use-case handlers, application services, commands, and queries.
   - Depends **only** on `domain`.
   - Coordinates domain models through repository ports without knowing their physical implementations.

3. **`infrastructure` (Adapters & Composition Root)**:
   - Implements ports defined in `domain` (e.g. `SqlxDocumentInstanceRepository`, `SqlxRoleRepository`).
   - Hosts primary/driving adapters (Axum HTTP handlers, routing, middleware) and secondary/driven adapters (SQLx pool, AWS SDK, Cognito, IAM token rotators).
   - Serves as the binary entry point (`src/main.rs`) and composition root.

### Dependency Rule
```
infrastructure ──► application ──► domain
      │                               ▲
      └───────────────────────────────┘ (implements domain ports)
```
Reverse dependencies are strictly prohibited.

---

## Considered Alternatives

1. **Monolithic single-crate architecture**:
   - *Pros*: Simpler cargo setup; fewer module boundaries.
   - *Cons*: Cannot prevent accidental leaking of SQL or HTTP types into business entities at compile time.
2. **Traditional layered N-tier architecture (`api` -> `service` -> `dao`)**:
   - *Pros*: Familiar to enterprise developers.
   - *Cons*: Core domain models depend directly on the database/ORM layer, making storage migrations and pure unit testing cumbersome.

---

## Consequences

### Positive
- Business rules are 100% pure and can be tested instantaneously without spin-up of databases, mocks, or HTTP servers.
- Storage technologies (e.g., PostgreSQL vs. AWS Aurora DSQL) can be adapted or swapped without touching domain or application code.
- Compile-time enforcement guarantees architectural boundaries cannot be violated.

### Negative
- Requires mapping between domain models, DTOs, and database record entities at adapter boundaries.
- Trait definitions and dependency injection boilerplate in the composition root.
