# Skill: Clean Code & Refactoring Guidelines (Backend)

Load this skill before designing, writing, refactoring, or reviewing backend Rust code.

---

## 1. Clean Code Principles

- **Single Responsibility Principle (SRP)**:
  - Each function and struct must have only one reason to change.
  - Never mix raw filesystem/database I/O, error formatting, data transformation, and business validation in a single function. Extract each concern into dedicated private helpers.
- **Single Level of Abstraction Principle (SLAP)**:
  - **Orchestrators**: Coordinate high-level workflow steps via helper functions; contain zero raw I/O, regex, or inline loop transformations.
  - **Workers**: Perform focused low-level tasks (e.g. read file bytes, parse numeric bound, build SeaQuery AST).
- **Function Size & Nesting**:
  - Target 10–30 lines per function (hard ceiling: 40 lines).
  - Max nesting depth: 2 levels. Use guard clauses (`?`, `let-else`) to fail fast at the top.
- **Command-Query Separation (CQS)**:
  - Methods either mutate state or return a query, not both (unless idiomatic in Rust collections, e.g. `insert`).
- **Newspaper Ordering**:
  - Code reads like a newspaper article: high-level entry points at the top, delegating downwards to leaf subroutines below, with tests at the bottom.

---

## 2. Structural & Trait Design Patterns

- **Guard Clauses**:
  - Validate preconditions, permissions, and invariants early with immediate returns (`return Err(...)`, `?`, `let ... else { return ... }`).
- **Interface Segregation & State Accessor Trait Hierarchy**:
  - Keep web driving state accessors segregated per capability (`HasDocumentsService`, `HasAuthResolver`, `HasHealthChecker`) so HTTP handlers depend only on what they consume.
  - Domain service accessors (`HasDocumentsService`, `HasAuthResolver`, etc.) inherit `HasSystemContext` as a baseline capability. This guarantees handlers automatically have access to schema and config lookups without redundant `+ HasSystemContext` bounds. Rust deduplicates overlapping supertraits at compile time with zero overhead.
  - Infrastructure probe traits (`HasHealthChecker`) remain deliberately decoupled from `HasSystemContext` to allow minimal probe states.
- **Rust Idioms & GoF Patterns**:
  - For Value Objects (`nutype`), Builders, Command Enums, Typestates, and Anti-Corruption Layer (ACL) codecs, refer to [`rust-patterns.md`](rust-patterns.md).

---

## 3. Session Enforcement Mechanics

1. **Pre-Coding**: Identify orchestrator vs. worker boundaries and plan focused helper functions.
2. **During Coding**: Maintain SLAP, SRP, and newspaper item ordering.
3. **Post-Coding (Mandatory)**: Automatically execute the full checklist in [`.ai/skills/common/review.md`](../common/review.md) before finishing.
