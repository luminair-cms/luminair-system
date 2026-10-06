# ADR-001: Declarative Behavior Specifications as the Source of Truth for Tests

- **Status**: Accepted
- **Date**: 2026-10-05 (proposed) · 2026-10-06 (accepted)
- **Deciders**: Dmitri Astafiev
- **Decision trigger**: the `application` crate (use cases) is implemented — reached; see [Decision](#decision)
- **Research**: [`docs/research/declarative-test-specifications.md`](../research/declarative-test-specifications.md) (detailed examples for the domain model)
- **Operational rules**: [`docs/specs/GUIDELINES.md`](../specs/GUIDELINES.md) and [`.ai/skills/spec-testing.md`](../../.ai/skills/spec-testing.md)

---

## Context

Per [`AGENTS.md`](../../AGENTS.md) §0, main code is written by hand, while **tests are written mostly by AI**.

That split has a structural flaw: when an AI derives tests by reading the implementation, it encodes
*what the code does*, not *what the code should do*. Bugs get frozen into assertions
("tautological tests"). The test suite is green, yet it verifies nothing that the author did not
already believe.

Concrete evidence from the `domain` crate (found while preparing this ADR on 2026-10-05).
**All three were fixed in code before acceptance** (2026-10-06); the table is kept as historical evidence of the failure mode.

| Location | Code said | Docs/comments said | State on 2026-10-06 |
|---|---|---|---|
| [`AuthorizationService::can`](../../domain/src/auth/service.rs) | Owner may only **read / update** without a role | Doc comment: owner may "read, update, **delete, publish**" | Fixed: owner may read/update/delete; **publish always needs RBAC** (documented) |
| [`DocumentInstance::unpublish`](../../domain/src/content/instance.rs) | Did not take `by`; `audit.updated_by` unchanged | No rule written down | Fixed: takes `by`, calls `touch` |
| [`AccessRequest::approve`](../../domain/src/auth/access_request.rs) | A reviewer could approve their **own** request | No rule written down | Fixed: self-approval rejected (`test_approve_by_self_fails`) |

A test generated from the code would have accepted any of these as correct. A test generated from a
written rule would either have confirmed the intent or flagged a bug. The fixes themselves were
verified by after-the-fact tests, so the underlying risk is **not** retired: the rest of the code
base is still covered by tests that were derived from the implementation.

We need a way for the human to state **expected behavior** once, briefly and precisely
(English or Russian), and let the AI generate the test code **from that statement**, not from the
implementation.

### Requirements

1. **R1 — Human-owned truth.** Expected behavior is written (or at least reviewed and approved) by a human, separately from the code.
2. **R2 — Short.** Writing a rule costs about as much as writing a sentence. Reviewing 20 rules takes minutes.
3. **R3 — Precise.** No ambiguity about inputs, the action, and the observable outcome (including the error kind).
4. **R4 — Traceable.** Every test maps to a rule; every rule has a test.
5. **R5 — Low glue cost.** Adding a new case should not need new Rust code most of the time.
6. **R6 — Fits the stack.** Plain `cargo test`; works with `domain::test_support`; no runtime dependencies in `domain`.
7. **R7 — Works for both layers.** Pure domain (synchronous) and application use cases (async, fake repositories).

---

## Considered Alternatives

### Option 1 — Gherkin / BDD with `cucumber-rs`

Write `.feature` files (Gherkin supports `# language: ru`) and implement the step definitions in Rust.

- ✅ Industry standard; natural language; Russian keywords supported natively.
- ✅ Good for long multi-step business flows (application layer).
- ❌ Every new phrasing needs a new regex step definition → glue code grows fast (R5).
- ❌ Needs a separate test harness (`harness = false`), an async runtime and a `World` type; heavy for pure domain functions (R6).
- ❌ Free text drifts: "document is published" vs "the document gets published" are different steps.

### Option 2 — Declarative scenario tables (YAML/TOML) + one generic runner per aggregate

Scenarios are data: `given` (initial state) → `when` (one action) → `then` (expected state or error kind).
One Rust runner per aggregate/service turns each scenario into a separate `cargo test` case.

- ✅ Zero Rust code per new scenario (R5); very fast to review in a diff (R2).
- ✅ Precise by construction: a closed vocabulary of actions and assertions; unknown keys fail parsing (R3).
- ✅ Excellent for **state machines** and **decision tables** (publication lifecycle, RBAC, access requests, content validation).
- ✅ The runner is small and written once; it can live in `domain/tests/` and reuse `test_support` (R6).
- ❌ The vocabulary (DSL) must be designed and kept in sync with the domain API.
- ❌ Bad fit for flows with branching logic or many steps; the runner turns into an interpreter.
- ❌ Errors are matched by *kind* (e.g. `InvalidStateTransition`), because `DomainError` is not `PartialEq`.

### Option 3 — Markdown rule specs + "blind" AI test generation

Behavior is written as numbered rules in `docs/specs/**.md` (English or Russian, Given/When/Then
style). The AI generates normal Rust `#[test]` functions **from the spec and public signatures only**,
without reading function bodies. Each test name carries the rule ID.

- ✅ No framework, no DSL, no runner: output is ordinary Rust tests using `test_support` (R6).
- ✅ Handles anything Rust can express: async use cases, fake repositories, multi-step flows (R7).
- ✅ Rule IDs give two-way traceability (R4); a spec diff shows exactly which behavior changed.
- ✅ Specs double as living documentation.
- ❌ "Blindness" is a process rule and needs discipline (or tooling) to enforce.
- ❌ Generated Rust tests still have to be reviewed (only lightly: check that each test matches its rule).
- ❌ Natural language is less strict than a table; rules must follow a fixed template (R3).

### Option 4 — Property-based testing (`proptest`) for invariants

State universal invariants ("revision never decreases", "constraint X rejects any string longer than N
chars") and let the framework generate inputs.

- ✅ Finds edge cases nobody thought of (unicode lengths, decimal scale, boundaries).
- ❌ Does not describe use cases; it complements Options 2 and 3 rather than replacing them.

---

## Comparison

| Criterion | 1 Gherkin | 2 Scenario tables | 3 Blind spec → Rust | 4 Proptest |
|---|---|---|---|---|
| R1 Human-owned truth | ✅ | ✅ | ✅ | ✅ (invariants) |
| R2 Short to write/review | ✅ | ✅✅ | ✅ | ✅ |
| R3 Precision | ⚠️ free text | ✅✅ closed vocabulary | ⚠️ needs template | ✅ |
| R4 Traceability | ✅ | ✅ | ✅ rule IDs | ⚠️ |
| R5 Low glue per case | ❌ | ✅✅ | ✅ (AI writes it) | ⚠️ |
| R6 Fits the stack | ⚠️ harness + runtime | ✅ | ✅✅ | ✅ |
| R7 Domain + use cases | ✅ | ⚠️ domain mainly | ✅✅ | ⚠️ domain mainly |
| Setup cost | Medium | Low–Medium | ~Zero | Low |

---

## Decision

Adopt a **hybrid of Option 3 (default) + Option 2 (state machines / decision tables)**, with Option 4
(`proptest`) for value-object invariants. Gherkin (Option 1) is rejected: it gives the readability of
Option 3 at the glue cost of a framework.

| Behavior kind | Format | Spec file | Test location | Example |
|---|---|---|---|---|
| Lifecycle / state machines | Option 2 case table | `docs/specs/domain/*.md` | `domain/tests/` | `DocumentInstance` publish/unpublish, `AccessRequest` approve/reject |
| Decision tables | Option 2 case table | `docs/specs/domain/*.md` | `domain/tests/` | `AuthorizationService::can`, `validate_content` |
| Use cases (application layer) | Option 3 rule → plain `#[tokio::test]` with fake repos | `docs/specs/application/*.md` | `application/tests/` | `DocumentsService::publish`, `AccessRequestsService::approve` |
| Value-object invariants | Option 4 `proptest` | `docs/specs/domain/*.md` (type `property`) | `domain/tests/` | `AttributeId`, `LocaleId`, `RegexPattern` |

### Resolved open questions

| Question | Decision | Why |
|---|---|---|
| Scenario file format (YAML vs TOML) | **Neither.** Case tables are plain Rust data (`const`/`vec!` of structs with closed enums) inside the test file. | No new dependency (`serde_yaml` is archived), no parser, no runner; the compiler rejects unknown keys and values (R3, R6); the DSL cannot grow beyond what Rust enums allow. Revisit only if tables exceed ~50 rows per aggregate. |
| Test-per-scenario harness (`libtest-mimic` vs loop) | **One `#[test]` per spec group that runs all cases and reports *all* failing rule IDs at once.** | Keeps plain `cargo test` (R6). A loop that stops at the first failure hides how many rules are red. |
| Enforcing "blind" generation | **Structure + a mechanical check + process.** Spec-backed tests live in `<crate>/tests/` (integration tests compile against the public API only, so they cannot depend on private details). A traceability test fails when an *Approved* rule ID has no test. The allowed-context list in the guidelines is a process rule. | A generated public-API summary file is extra tooling for little gain: the public API is already visible to the compiler. |

### Where tests live (inline vs separate)

| Kind of test | Location | Reason |
|---|---|---|
| **Spec-backed** (rules in `docs/specs/`) | `<crate>/tests/*.rs` | Public API only: guarantees the test checks observable behavior, supports blind generation, and keeps AI-generated code out of production files. |
| Existing tests | Stay inline as they are | No churn; migrate to spec-backed tests when the related code changes. |
| Tests of **private** invariants / helpers that have no public surface | Inline `#[cfg(test)]` | Cannot be reached from `tests/`. Not spec-backed; mark as such in a comment if there is any doubt. |
| Fakes of `application::test_support` | Inline self-tests in the same file | A fake that diverges from the port contract invalidates every use-case spec built on it. |

Consequence for the build: `domain` gets the same self dev-dependency with `test-support` that
`application` already has, so `domain/tests/` can use `domain::test_support`.

### Workflow rules (apply to every option)

1. **Spec first.** A behavior change starts with a spec diff, reviewed by a human. Specs are written by AI *or* human; **only a human sets `Status: Approved`**.
2. **Blind generation.** The AI that generates tests receives only: the approved spec, public signatures (`cargo doc` / `pub` items), and `test_support`. It does not read function bodies or existing tests of the unit under test.
3. **Red is a finding, not a typo.** If a generated test fails, the AI reports a *spec–code mismatch* and does not edit the assertion. The human decides whether the code or the spec is wrong.
4. **Traceability.** The rule ID appears in the test (test name for Option 3 / `property`, `id` field for Option 2). `domain/tests/spec_traceability.rs` enforces this for `Approved` rules.
5. **Existing tests** stay as they are and migrate gradually (see the table above).

---

## Consequences

### Positive
- Tests verify intent; spec–code mismatches like the ones in *Context* surface automatically.
- Writing tests by hand is mostly replaced by writing short rules.
- Specs become reviewable, versioned business documentation.

### Negative / Risks
- Two artefact types (markdown specs + Rust case tables) to keep in sync; the traceability check only proves that a rule ID is *mentioned* in a test, not that the test is right. Human review of the generated test against the rule is still required (see the review guidelines).
- Case tables can grow into a mini-language; keep them to one action per case.
- Blind generation depends on discipline; it is weakened if the AI is given the whole repository as context.
- Fakes are a second implementation of the ports. Use-case specs are only as good as the fakes, hence their self-tests.
- Specs written by AI from the current behavior carry the same tautology risk if the human approves them without reading. The review guidelines therefore require the reviewer to answer "is this what the system *should* do?", not "is this what it does?".

### Follow-ups
- [x] `docs/specs/` structure, template, guidelines and examples (`docs/specs/`).
- [x] Skill for generating and reviewing tests from specs: [`.ai/skills/spec-testing.md`](../../.ai/skills/spec-testing.md); [`.ai/skills/testing.md`](../../.ai/skills/testing.md) updated.
- [x] Fakes: `FieldFilter` honored, lock errors reported as `Storage`, deterministic paging.
- [x] Initial draft specs (all `Draft`, awaiting human review).
- [ ] Human review and approval of the draft specs, then blind test generation per spec file.
- [ ] Pilot: generate tests for `LC` (`DocumentInstance` lifecycle) and compare with existing inline tests.
- [ ] Log the decision in `.ai/context/decisions.md` (done together with this ADR).
