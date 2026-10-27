# Skill: Code Review Checklist

> **MANDATORY**: Run this checklist **automatically after every coding session** before concluding the task or presenting results.

## 1. Automated Gates (must all pass)

```bash
cargo clippy -- -D warnings
cargo test --workspace
cargo audit
```

## 2. Architecture & Design Patterns ([`clean-code.md`](clean-code.md), [`rust-patterns.md`](rust-patterns.md), [`docs/architecture.md`](../../docs/architecture.md))

- [ ] Hexagonal boundaries: `domain` and `application` contain zero I/O, SQL, or HTTP imports
- [ ] Repository traits defined in `domain`, implemented only in `infrastructure`
- [ ] Anti-Corruption Layer (ACL): Wire DTOs strictly separated from domain types
- [ ] Facade pattern: Internal worker modules hidden via `mod` / `pub(crate)`; only cohesive entry points and public types re-exported via `pub use`
- [ ] Idiomatic Rust patterns: Builders for complex construction, Newtypes for type safety, Strategy closures/traits, Enum State/Command, `From`/`TryFrom` factories, `OnceLock`/`LazyLock` or startup `&'static` instead of mutable statics
- [ ] No anti-patterns: No `Deref` inheritance, no unnecessary `.clone()` to silence the borrow checker, no `#![deny(warnings)]` in library crates

## 3. Clean Code (SRP & SLAP) ([`clean-code.md`](clean-code.md))

- [ ] **SRP**: Each function and struct has one reason to change; auxiliary logic extracted
- [ ] **SLAP**: Orchestrators only coordinate; workers handle raw I/O, parsing, and loops
- [ ] **Function Size**: 10–30 lines target (flag > 40 lines), nesting depth $\le 2$, guard clauses fail fast

## 4. Item Ordering & Idioms ([`rust-patterns.md`](rust-patterns.md))

- [ ] Types first (constants, structs, enums) before functions and `impl`s
- [ ] Public functions first in dependency order ("newspaper style"); private helpers after
- [ ] Explicit conversions: `TargetType::from(x)` used over `.into()` when target type is ambiguous
- [ ] Exact numbers: `rust_decimal::Decimal` used instead of floats in domain models
- [ ] `#[cfg(test)] mod tests` at the bottom of the file

## 5. Error Handling

- [ ] No `.unwrap()` or `.expect()` outside of tests; all errors use `thiserror`
- [ ] HTTP error mapping centralized in infrastructure middleware

## 6. API Surface ([`docs/api.md`](../../docs/api.md))

- [ ] Any endpoint change reflected in `docs/api.md`
- [ ] Standard response envelope and RFC 9457 `application/problem+json` returned

## 7. Data Model

- [ ] Primary keys use `Uuid::now_v7()` (no auto-increment/sequences)
- [ ] Migrations prefixed with timestamp; strictly `-- no-transaction` for Aurora DSQL

## 8. Security

- [ ] Zero secrets committed; inputs validated before domain logic; no SQL string interpolation

## 9. Tests ([`testing.md`](testing.md), [`spec-testing.md`](spec-testing.md))

- [ ] Domain logic has unit/property tests; use-cases tested with fakes; endpoints tested via integration tests
- [ ] Spec-backed tests live in `<crate>/tests/` and pass `spec_traceability`

## 10. Documentation

- [ ] Public items have `///` doc comments; ADR created for architectural decisions; `.ai/context/decisions.md` updated
