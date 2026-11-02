# Common Workspace Rules

These non-negotiable rules and invariant guardrails apply unconditionally to all code, workflows, and agents across this entire repository.

---

## 1. Human Governance & Approval Boundaries

- **Human Owns Intent**:
  - The AI agent **must NEVER mark a behavior specification as `Approved`** (only `Draft`). Only humans possess the authority to approve specifications.
  - The AI agent **must NEVER mark an Architecture Decision Record (ADR) as `Accepted`** (only `Proposed`). Only humans approve architectural decisions.
  - Speculative or unapproved architectural changes must never be implemented without an accepted ADR or explicit user instruction.
- **Specification-Driven Testing Integrity**:
  - **Blind Generation**: When generating tests from an approved specification, the agent is strictly forbidden from reading function implementation bodies or internal inline tests. Only approved spec rules, public signatures, and `test_support` are allowed.
  - **Spec–Code Mismatch**: If a spec-backed test fails against code, the agent must **NEVER** edit the assertion or the rule to make it pass. It must immediately halt and report a `SPEC–CODE MISMATCH DETECTED` for human decision.

---

## 2. Layering, Decoupling & Hexagonal Boundaries

- **Pure Business Logic**: Core domain and business logic must remain decoupled from external wire formats, network protocols, and specific database engines.
- **Anti-Corruption Layer (ACL)**: External representations (REST DTOs, database rows, configuration schemas) must never bleed directly into domain models. Explicit codecs, mappers, or builders must translate across layer boundaries.

---

## 3. Canonical Error Schema & API Contracts (RFC 9457)

- **Problem Details Standard**: All non-2xx HTTP responses must return `Content-Type: application/problem+json` conforming to RFC 9457 (see [`docs/api.md`](../../docs/api.md)).
- **Decoupled Localization**:
  - The **backend** returns a stable, machine-readable `code` (e.g. `draft_has_pending_edits`), a developer English diagnostic `detail`, and structured parameters in `details`.
  - The **frontend** owns end-user localization: it maps `error.code` to localized copy (`t('errors.' + error.code, error.details)`). Frontend code must never display raw backend developer `detail` messages as primary end-user copy.
- **API Surface Documentation**: Any endpoint change, parameter addition, or error code alteration must update [`docs/api.md`](../../docs/api.md) in the same change.

---

## 4. Security & Safety

- **Zero Secrets**: Never commit secrets, tokens, credentials, or private configuration to version control.
- **No SQL String Concatenation**: All database queries must be strictly parameterized through AST builders or query engines (e.g. `sea-query`, `sqlx`). Dynamic string interpolation in SQL is strictly forbidden.
- **Boundary Validation**: Validate all external user inputs before delegating to domain logic.

---

## 5. Mandatory Verification & Review Gates

- **Automatic Post-Coding Self-Review**: Immediately after modifying, refactoring, or writing any code in *any* session, the agent **must automatically execute the review checklist** in [`.ai/skills/common/review.md`](../skills/common/review.md) before concluding the task or presenting results.
- **Automated Gates**: All workspace checks must pass with zero warnings or errors before concluding:
  - Backend: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
  - Frontend: `npm run lint` and `tsc --noEmit` (when `frontend/` is present)
