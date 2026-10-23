# Skill: Clean Code & Design Patterns for Luminair

Load this skill before designing, writing, refactoring, or reviewing code.

---

## 1. Clean Code Principles

- **Single Responsibility Principle (SRP)**: One reason to change. Never mix filesystem/database I/O, error formatting, data transformation, and business validation in one function. Extract each concern into dedicated helpers.
- **Single Level of Abstraction Principle (SLAP)**:
  - **Orchestrators**: Coordinate high-level steps via helper functions; contain zero raw I/O or inline loop transformations.
  - **Workers**: Perform focused low-level tasks (e.g. read file bytes, parse numeric bound).
- **Function Size & Nesting**: Target 10–30 lines per function (flag > 40 lines). Max nesting depth: 2 levels. Use guard clauses (`?`) to fail fast at the top.
- **Command-Query Separation (CQS)**: Methods either mutate state or return a query, not both (unless idiomatic in Rust, e.g. `insert`).
- **Newspaper Ordering**: Step-down order (entry points at the top, leaf helpers below, tests at bottom). See [`.ai/skills/rust-patterns.md`](rust-patterns.md).

---

## 2. Design Patterns in Luminair

- **Hexagonal Architecture**: Pure `domain` and `application` logic; all I/O, SQL, and HTTP isolated in `infrastructure`. See [`docs/architecture.md`](../../docs/architecture.md).
- **Anti-Corruption Layer (ACL)**: Isolate wire formats (Serde JSON, DTOs) from domain entities. Never bleed external schema quirks into `domain`.
- **Value Objects & Newtypes**: Enforce domain invariants at construction via `nutype`. See [`.ai/skills/rust-patterns.md`](rust-patterns.md).
- **Builder Pattern**: Use fluent builders for complex domain entities and test fixtures (e.g. `DocumentTypeBuilder`).
- **Guard Clauses**: Validate preconditions and permissions early with immediate returns.

---

## 3. Session Enforcement Mechanics

1. **Pre-Coding**: Identify orchestrator vs. worker boundaries and plan focused helper functions.
2. **During Coding**: Maintain SLAP and newspaper item ordering.
3. **Post-Coding (Mandatory)**: Automatically execute the full checklist in [`.ai/skills/review.md`](review.md) before finishing.
