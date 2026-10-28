# AGENTS.md — AI Agent Entry Point

> This file is the **single entry point** for any AI coding agent working in this repository.
> Read it first, follow the links in order, then begin your task.

## 0. Division of responsibilities

Main code is written with help of AI (review, analysis, code snippets etc).

Documentation and Tests are written mostly by AI.

## 1. Understand the Project

Read in this order:
1. [`README.md`](./README.md) — what, why, quick-start
2. [`docs/architecture.md`](./docs/architecture.md) — crate layout, hexagonal architecture, DDD
3. [`docs/design-infrastructure-persistence.md`](./docs/design-infrastructure-persistence.md) — persistence design and DSQL patterns
4. [`docs/content-lifecycle.md`](./docs/content-lifecycle.md) — document states, publish/unpublish/restore semantics, snapshot rules (diagrams + full transition matrix)
5. [`docs/ui-architecture.md`](./docs/ui-architecture.md) — admin studio UI architecture
6. [`docs/api.md`](./docs/api.md) — REST API endpoints, canonical RFC 9457 error schema, and UI localization guide
7. [`docs/plan-infrastructure.md`](./docs/plan-infrastructure.md) — infrastructure crate module structure, composition root, and step-by-step implementation plan

*(Note: `docs/roadmap.md` and `docs/data-model.md` are planned milestone artifacts).*

## 2. Load Skills Before Acting

| Task | Skill to load |
|---|---|
| Writing or modifying Rust code | [`.ai/skills/clean-code.md`](.ai/skills/clean-code.md), [`.ai/skills/rust-patterns.md`](.ai/skills/rust-patterns.md) |
| Writing or modifying tests | [`.ai/skills/testing.md`](.ai/skills/testing.md), [`.ai/skills/spec-testing.md`](.ai/skills/spec-testing.md) |
| Reviewing / self-reviewing code | [`.ai/skills/review.md`](.ai/skills/review.md), [`.ai/skills/clean-code.md`](.ai/skills/clean-code.md), [`.ai/skills/rust-patterns.md`](.ai/skills/rust-patterns.md) |
| Investigating a topic / drafting an ADR | [`.ai/skills/adr-process.md`](.ai/skills/adr-process.md) |

## 3. Memory (Persistent Context)

- [`.ai/context/project.md`](.ai/context/project.md) — one-paragraph seed for cold-start sessions
- [`.ai/context/decisions.md`](.ai/context/decisions.md) — running log of key decisions (informal complement to ADRs)
- [`docs/research/`](./docs/research/) — investigation/spike notes (raw findings, no opinions)
- [`docs/adr/`](./docs/adr/) — Architecture Decision Records; use [`ADR-001`](./docs/adr/ADR-001-declarative-test-specifications.md) as the format reference for new ones

### ADR Index

| ADR | Title | Status |
|---|---|---|
| [ADR-001](./docs/adr/ADR-001-declarative-test-specifications.md) | Declarative behavior specifications as the source of truth for tests | Accepted |
| [ADR-002](./docs/adr/ADR-002-hexagonal-architecture.md) | Hexagonal Architecture and DDD Crate Layout | Accepted |
| [ADR-003](./docs/adr/ADR-003-aws-dsql-database.md) | AWS Aurora DSQL with PostgreSQL Portability | Accepted |
| [ADR-004](./docs/adr/ADR-004-draft-and-publish-table-pattern.md) | Asymmetric Hybrid Draft-and-Publish Persistence Pattern | Accepted |
| [ADR-005](./docs/adr/ADR-005-trash-undelete-and-retention.md) | Trash, Undelete & Retention Architecture | Proposed |
| [ADR-006](./docs/adr/ADR-006-user-profile-and-directory-architecture.md) | User Profile and Directory Architecture for External Identity Management | Proposed |

## 4. Mandatory Workflow Rules

- **Automatic Post-Coding Self-Review**: Immediately after modifying, refactoring, or writing any code in *any* session, the agent **must automatically execute the review checklist** in [`.ai/skills/review.md`](.ai/skills/review.md) before concluding the task or presenting results.
- **Clean Code & SLAP/SRP**: All code must strictly conform to Single Responsibility Principle (SRP) and Single Level of Abstraction Principle (SLAP) as defined in [`.ai/skills/clean-code.md`](.ai/skills/clean-code.md). High-level orchestrators must delegate details to focused helper functions (target 10–30 lines per function, max nesting depth 2).
- **All code must pass**: `cargo clippy -- -D warnings` and `cargo test --workspace`
- **Code Organization & Ordering**: Follow "newspaper style" item ordering from [`.ai/skills/rust-patterns.md`](.ai/skills/rust-patterns.md): types first, functions/impls after; public functions first in dependency order, private helpers after; tests at the bottom.
- **New endpoints**: add an entry to `docs/api.md`
- **Architecture changes**: create a new ADR in `docs/adr/` following [`.ai/skills/adr-process.md`](.ai/skills/adr-process.md) (next free number; structure as in [`ADR-001`](./docs/adr/ADR-001-declarative-test-specifications.md)) and add it to the ADR index above
- **Generating tests**: never derive expected behavior from the implementation; if a test fails, report it as a spec–code mismatch instead of changing the assertion (see [`ADR-001`](./docs/adr/ADR-001-declarative-test-specifications.md))
- **Never put domain logic in `infrastructure` crate** — see architecture doc

## 5. Sensitive / Local Files

`.ai/local/` is gitignored. Put any session-specific secrets, tokens, or personal context there.
Never commit anything from `.ai/local/`.
