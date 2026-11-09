# Skill: Universal Code Review Checklist & Scorecard

Load this skill when performing a pre-commit or post-coding self-review of code, or when auditing pull requests.

---

## 1. Automated Gates (Must All Pass)

### Backend (Rust)
```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit
```

### Frontend (React / TypeScript)
```bash
npm run lint
tsc --noEmit
npm test
```

---

## 2. Architecture & Layering ([`common.md`](../../rules/common.md), [`backend-rust.md`](../../rules/backend-rust.md), [`docs/architecture.md`](../../../docs/architecture.md))

- [ ] Hexagonal boundaries: `domain` and `application` contain zero I/O, SQL, or HTTP imports.
- [ ] Repository traits defined in `domain`, implemented only in `infrastructure`.
- [ ] Anti-Corruption Layer (ACL): Wire DTOs strictly separated from domain types.
- [ ] Facade pattern: Internal worker modules hidden via `mod` / `pub(crate)`; only cohesive entry points and public types re-exported via `pub use`.
- [ ] Concurrency: `tokio::spawn` restricted to `infrastructure`; no task spawning in `domain` or `application`.

---

## 3. Clean Code (SRP & SLAP) ([`clean-code.md`](../backend/clean-code.md))

- [ ] **SRP**: Each function and struct has one reason to change; auxiliary logic extracted into helpers.
- [ ] **SLAP**: Orchestrators only coordinate; workers handle raw I/O, parsing, loops, or SQL AST building.
- [ ] **Function Metrics**: 10–30 lines target (flag > 40 lines), nesting depth $\le 2$, guard clauses fail fast.

---

## 4. Rust Idioms & Ordering ([`rust-patterns.md`](../backend/rust-patterns.md))

- [ ] Top-down newspaper ordering: Types first, functions in dependency order, `#[cfg(test)] mod tests` at the bottom.
- [ ] Value Objects: `nutype` used for all domain identifiers and constrained scalar values.
- [ ] Exact numbers: `rust_decimal::Decimal` used instead of floats in domain models.
- [ ] Primary keys: `Uuid::now_v7()` used (no auto-increment or sequences).
- [ ] Error handling: Zero `.unwrap()` or `.expect()` outside of tests; all errors use `thiserror`.
- [ ] No anti-patterns: No `Deref` inheritance simulation, no ungrounded `.clone()` to silence the borrow checker, no `#![deny(warnings)]` in library crates.

---

## 5. Frontend Architecture & Design System ([`frontend-react.md`](../../rules/frontend-react.md), [`react-patterns.md`](../frontend/react-patterns.md))

- [ ] Strict TypeScript: Zero `any`; all props and hook return types explicitly typed.
- [ ] Mantine components used; zero raw CSS or inline styles where Mantine style props or theme tokens exist.
- [ ] React Compiler & React 19 compliance: zero manual `useMemo`/`useCallback` for performance; render functions strictly pure; zero compiler diagnostic suppressions; `ref` passed as a regular prop (no `forwardRef`).
- [ ] Server state managed via TanStack Query v5 custom hooks (zero data fetching in `useEffect`).
- [ ] Client state properly separated: local state in `useState`, global session/draft state in Zustand.
- [ ] RFC 9457 error mapping: `error.code` mapped to translation dictionary (`t('errors.' + error.code)`); backend developer `detail` never displayed directly as localized user copy.
- [ ] Validation errors (`invalid_params`) mapped to `@mantine/form` field error states.

---

## 6. API Surface & Localization ([`docs/api.md`](../../../docs/api.md))

- [ ] Any endpoint change reflected in `docs/api.md`.
- [ ] Standard response envelope and RFC 9457 `application/problem+json` returned on all errors.

---

## 7. Security & Input Validation

- [ ] Zero secrets, keys, or credentials committed.
- [ ] All database queries parameterized; zero SQL string formatting/interpolation.
- [ ] All external user inputs validated before reaching domain logic.

---

## 8. Tests & Specification Traceability ([`testing.md`](../backend/testing.md), [`spec-testing.md`](spec-testing.md))

- [ ] Domain logic has unit/property tests; use-cases tested with fakes; endpoints tested via integration tests.
- [ ] Spec-backed tests live in `<crate>/tests/*.rs` and pass `spec_traceability`.
- [ ] Spec assertions were never weakened or edited to make failing code pass.
- [ ] Frontend components tested with Vitest + React Testing Library using MSW for API mocking.

---

## 9. Documentation & Decisions ([`adr-process.md`](adr-process.md))

- [ ] Public items have `///` doc comments.
- [ ] Architectural decisions documented in `docs/adr/` with human approval and indexed in `AGENTS.md` and `.ai/context/decisions.md`.
