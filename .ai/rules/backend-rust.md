# Backend (Rust) Rules

These non-negotiable rules and invariants govern all Rust code across `domain`, `application`, `infrastructure`, and associated tests.

---

## 1. Hexagonal Crate Boundaries

- **Pure Domain & Application**: `domain` and `application` crates must contain pure business logic and orchestration with **ZERO** SQL (`sqlx`), HTTP (`axum`), or filesystem I/O dependencies.
- **Port Placement**:
  - Repository traits (`trait DocumentInstanceRepository`, `trait RoleRepository`) must be declared in `domain`.
  - Repository implementations live exclusively in `infrastructure`.
- **Runtime Concurrency**: `tokio::spawn` and background concurrency belong exclusively in `infrastructure`. Never spawn tasks in `domain` or `application`.

---

## 2. AWS Aurora DSQL & Persistence Invariants

- **Primary Keys**: Always use time-ordered `uuid::Uuid::now_v7()`. Never use auto-increment integers or database sequences (unsupported by AWS Aurora DSQL).
- **Database Migrations**:
  - Migration filenames must be prefixed with a timestamp (`YYYYMMDDHHMMSS_name.sql`).
  - Migrations must be marked strictly `-- no-transaction` because Aurora DSQL does not support transactional DDL.

---

## 3. Type Safety & Value Objects

- **Mandatory `nutype` for Domain Primitives**:
  - Every domain identifier (`DocumentTypeId`, `RelationId`, `AttributeId`, `UserId`) and constrained value object (`LocaleId`, `SlugValue`, `Email`) **must** use `nutype` or strongly-typed UUID newtypes.
  - Never pass raw `String`, `Uuid`, or `i64` in domain signatures or entity fields.
- **Exact Numerics**:
  - Always use `rust_decimal::Decimal` for financial amounts, numeric bounds, and schema precision numbers.
  - **Never use `f32` or `f64`** in domain models or calculations.
- **Deterministic Collections**:
  - Use `indexmap::IndexMap` and `indexmap::IndexSet` whenever schema field order, attribute order, or insertion order must be preserved deterministically.

---

## 4. Strict Prohibitions & Anti-Patterns

- **Zero `.unwrap()` or `.expect()` in Production Code**:
  - Never call `.unwrap()` or `.expect()` in non-test code. Propagate errors using `?` with `thiserror`.
- **No `Deref` Polymorphism**:
  - Never implement `Deref` to simulate inheritance or object hierarchies. Use composition, explicit traits, or delegation methods.
- **No `.clone()` to Silence the Borrow Checker**:
  - Do not call `.clone()` simply to satisfy the borrow checker without explicit design justification. Prefer borrowed slices (`&str`, `&[T]`), moving owned values, or deliberate `Arc` / startup `&'static` references.
- **No `#![deny(warnings)]` in Crates**:
  - Enforce warnings strictly via CI tooling (`cargo clippy -- -D warnings`), never via crate attributes that break downstream builds on compiler updates.

---

## 5. Code Metrics & File Layout (Clean Code & SLAP)

- **Function Size & Nesting**:
  - Target: **10–30 lines** per function.
  - Hard Ceiling: **40 lines maximum**. Flag and split functions exceeding 40 lines.
  - Max Nesting Depth: **2 levels**. Use guard clauses (`?`, `let-else`) to fail fast at the top.
- **Single Level of Abstraction (SLAP)**:
  - Orchestrators coordinate high-level steps; workers perform low-level parsing, SQL AST building, or loops.
- **Newspaper File Layout**:
  Every `.rs` file must follow top-down dependency ordering:
  1. **Imports**: `std` → external crates → internal crates (`domain`, `application`, `crate::...`)
  2. **Types**: Constants, statics, type aliases, structs, enums
  3. **Implementations**: Inherent `impl`s first, trait `impl`s second
  4. **Functions**: Public entry points/orchestrators first, private leaf helpers after
  5. **Tests**: `#[cfg(test)] mod tests { ... }` at the bottom of the file

---

## 6. Testing Boundaries

- **Test Placement**:
  - Spec-backed, observable tests must live in integration test targets (`<crate>/tests/*.rs`), testing through public APIs and test fakes only.
  - Inline `#[cfg(test)] mod tests` in `src/` is strictly for private internal algorithms and helper invariants.
- **Zero Infrastructure Bleed**:
  - Domain and application tests must **never** import `sqlx`, live database pools, or infrastructure types. Use in-memory fakes from `application::test_support`.
- **Assertion Preservation**:
  - If a spec-backed test fails against code, **NEVER** edit assertions to match unexpected code behavior. Report a `SPEC–CODE MISMATCH` immediately.
