# ADR-001: Declarative Behavior Specifications as the Source of Truth for Tests

- **Status**: Proposed
- **Date**: 2026-10-05
- **Deciders**: Dmitri Astafiev
- **Decision deadline**: after the `application` crate (use cases) is implemented
- **Research**: [`docs/research/declarative-test-specifications.md`](../research/declarative-test-specifications.md) (detailed examples for the domain model; use-case examples will be added later)

---

## Context

Per [`AGENTS.md`](../../AGENTS.md) §0, main code is written by hand, while **tests are written mostly by AI**.

That split has a structural flaw: when an AI derives tests by reading the implementation, it encodes
*what the code does*, not *what the code should do*. Bugs get frozen into assertions
("tautological tests"). The test suite is green, yet it verifies nothing that the author did not
already believe.

Concrete evidence from the current `domain` crate (found while preparing this ADR):

| Location | Code says | Docs/comments say |
|---|---|---|
| [`AuthorizationService::can`](../../domain/src/auth/service.rs) | Owner may only **read / update** without a role | Doc comment: owner may "read, update, **delete, publish**" |
| [`DocumentInstance::unpublish`](../../domain/src/content/instance.rs) | Does not take `by`; `audit.updated_by` is not changed | No rule written down for it |
| [`AccessRequest::approve`](../../domain/src/auth/access_request.rs) | A reviewer can approve their **own** request | No rule written down for it |

A test generated from the code would accept any of these as correct. A test generated from a
written rule would either confirm the intent or flag a bug.

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

## Proposed Direction (to be confirmed after the `application` crate exists)

A **hybrid of Option 3 (default) + Option 2 (for state machines / decision tables)**, with Option 4
used selectively for value-object invariants:

| Behavior kind | Format | Example |
|---|---|---|
| Lifecycle / state machines | Option 2 scenario table | `DocumentInstance` publish/unpublish, `AccessRequest` approve/reject |
| Decision tables | Option 2 scenario table | `AuthorizationService::can`, `validate_content` |
| Use cases (application layer) | Option 3 markdown rules → Rust tests with fake repos | `PublishDocument`, `ApproveAccessRequest` |
| Value-object invariants | Option 4 `proptest` (optional) | `AttributeId`, `LocaleId`, `RegexPattern` |

Gherkin (Option 1) is not proposed: it gives the readability of Option 3 at the glue cost of a framework.

### Workflow rules (apply to any chosen option)

1. **Spec first.** A behavior change starts with a spec/scenario diff, reviewed by a human.
2. **Blind generation.** When the AI generates tests, it gets the spec plus public signatures / `test_support` only, never the function bodies.
3. **Red is a finding, not a typo.** If a generated test fails, the AI reports it as a *spec–code mismatch* and does not edit the assertion. The human decides whether the code or the spec is wrong.
4. **Traceability.** Test name or scenario `id` = rule ID (e.g. `LC-03`). CI may check that each rule ID appears in at least one test.
5. **Existing tests** stay as they are; they migrate to spec-backed tests gradually, when the related code changes.

---

## Consequences (if the proposed direction is accepted)

### Positive
- Tests verify intent; spec–code mismatches like the ones in *Context* surface automatically.
- Writing tests by hand is mostly replaced by writing short rules.
- Specs become reviewable, versioned business documentation.

### Negative / Risks
- Two artefact types (markdown specs + scenario files) and one runner per aggregate to maintain.
- The scenario DSL can grow into a mini-language; keep it limited to one action per scenario.
- Blind generation depends on discipline; it is weakened if the AI is given the whole repository as context.

### Follow-ups (after acceptance)
- Add `docs/specs/` structure and a spec template.
- Add the scenario runner for one aggregate (pilot: `DocumentInstance` lifecycle).
- Extend [`.ai/skills/testing.md`](../../.ai/skills/testing.md) with the blind-generation protocol.
- Add use-case examples to the research note once the `application` crate is implemented.
- Log the decision in `.ai/context/decisions.md`.

## Open Questions
- Scenario file format: YAML (`serde_yaml` is archived/unmaintained; maintained forks exist) vs TOML (`toml` crate, well-maintained but more verbose for nested data).
- Test-per-scenario harness: `libtest-mimic` / `datatest-stable` (one test per scenario, `harness = false`) vs a single `#[test]` that loops and aggregates failures.
- How to enforce "blind" generation: instructions only, or a generated public-API summary file used as the only code context.
