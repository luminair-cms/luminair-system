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
8. [`docs/e2e-testing.md`](./docs/e2e-testing.md) — end-to-end component testing architecture using Testcontainers (`postgres`, `dex`, and `luminair-cli:local`)

*(Note: `docs/roadmap.md` and `docs/data-model.md` are planned milestone artifacts).*

## 2. Rules & Skills Index

Rules are **mandatory invariants** that must never be violated. Skills are **on-demand runbooks** loaded when performing specific tasks.

### A. Modular Rules

- **Common Rules**: [`.ai/rules/common.md`](.ai/rules/common.md) — Human governance, hexagonal boundaries, RFC 9457 error contracts, security.
- **Backend (Rust) Rules**: [`.ai/rules/backend-rust.md`](.ai/rules/backend-rust.md) — Strict domain purity, `nutype`, `Decimal`, `Uuid::now_v7()`, zero `.unwrap()`, SLAP metrics, `cargo clippy`.
- **Frontend (React) Rules**: [`.ai/rules/frontend-react.md`](.ai/rules/frontend-react.md) — Strict TypeScript, Mantine design system, TanStack Query/Router, React Compiler invariants, RFC 9457 i18n localization.

### B. Skills by Domain

| Domain | Task | Skill to load |
|---|---|---|
| **Common** | Investigating a topic / drafting an ADR | [`.ai/skills/common/adr-process.md`](.ai/skills/common/adr-process.md) |
| **Common** | Behavior specification & mismatch handling | [`.ai/skills/common/spec-testing.md`](.ai/skills/common/spec-testing.md) |
| **Common** | Reviewing / self-reviewing code | [`.ai/skills/common/review.md`](.ai/skills/common/review.md) |
| **Backend** | Writing or refactoring clean Rust code | [`.ai/skills/backend/clean-code.md`](.ai/skills/backend/clean-code.md) |
| **Backend** | Rust idioms, GoF patterns & type design | [`.ai/skills/backend/rust-patterns.md`](.ai/skills/backend/rust-patterns.md) |
| **Backend** | Writing Rust tests (unit, fake repos, sqlx) | [`.ai/skills/backend/testing.md`](.ai/skills/backend/testing.md) |
| **Frontend** | React 19, Mantine, Query & Zustand patterns | [`.ai/skills/frontend/react-patterns.md`](.ai/skills/frontend/react-patterns.md) |
| **Frontend** | Frontend component & hook testing (Vitest, RTL, MSW) | [`.ai/skills/frontend/react-testing.md`](.ai/skills/frontend/react-testing.md) |

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
| [ADR-006](./docs/adr/ADR-006-user-profile-and-directory-architecture.md) | User Profile and Directory Architecture for External Identity Management | Accepted |

## 4. Mandatory Workflow Rules

- **Automatic Post-Coding Self-Review**: Immediately after modifying, refactoring, or writing any code in *any* session, the agent **must automatically execute the review checklist** in [`.ai/skills/common/review.md`](.ai/skills/common/review.md) before concluding the task or presenting results.
- **Clean Code & SLAP/SRP**: All code must strictly conform to Single Responsibility Principle (SRP) and Single Level of Abstraction Principle (SLAP) as defined in [`.ai/rules/backend-rust.md`](.ai/rules/backend-rust.md) and [`.ai/skills/backend/clean-code.md`](.ai/skills/backend/clean-code.md). High-level orchestrators must delegate details to focused helper functions (target 10–30 lines per function, max nesting depth 2).
- **All code must pass**:
  - Rust: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
  - Frontend: `npm run lint` and `tsc --noEmit`
- **Code Organization & Ordering**: Follow "newspaper style" top-down item ordering from [`.ai/rules/backend-rust.md`](.ai/rules/backend-rust.md) and [`.ai/rules/frontend-react.md`](.ai/rules/frontend-react.md): primary public entry points, components, and orchestrators first in dependency order, followed by private leaf helpers and workers below; tests at the bottom.
- **New endpoints**: add an entry to `docs/api.md` matching RFC 9457 error format and standard response envelopes.
- **Architecture changes**: create a new ADR in `docs/adr/` following [`.ai/skills/common/adr-process.md`](.ai/skills/common/adr-process.md) (next free number; structure as in [`ADR-001`](./docs/adr/ADR-001-declarative-test-specifications.md)) and add it to the ADR index above. Only humans can accept ADRs.
- **Generating tests**: never derive expected behavior from the implementation; if a test fails, report it as a spec–code mismatch instead of changing the assertion (see [`.ai/rules/common.md`](.ai/rules/common.md) and [`ADR-001`](./docs/adr/ADR-001-declarative-test-specifications.md)). Only humans can approve specification rules.
- **Never put domain logic in `infrastructure` crate** — pure business logic in `domain` and `application`.

## 5. Sensitive / Local Files

`.ai/local/` is gitignored. Put any session-specific secrets, tokens, or personal context there.
Never commit anything from `.ai/local/`.
