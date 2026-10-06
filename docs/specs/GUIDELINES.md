# Behavior Specs — Guidelines

> Operational rules for [ADR-001](../adr/ADR-001-declarative-test-specifications.md).
> Audience: **humans** and **AI agents**. Each part has a section for both.
> Worked examples: [`examples.md`](./examples.md). Template: [`TEMPLATE.md`](./TEMPLATE.md).
> Agent entry point: [`.ai/skills/spec-testing.md`](../../.ai/skills/spec-testing.md).

## 0. Overview

```
 ┌────────────┐   ┌──────────────┐   ┌─────────────┐   ┌──────────────┐   ┌───────────┐
 │ A. WRITE   │ → │ B1. REVIEW   │ → │ APPROVE     │ → │ C. GENERATE  │ → │ B2. REVIEW│
 │ spec rules │   │ the spec     │   │ (human only)│   │ tests, blind │   │ the tests │
 └────────────┘   └──────────────┘   └─────────────┘   └──────────────┘   └─────┬─────┘
        ▲                                                                         │
        └──────── red test = spec–code mismatch, human decides who is wrong ◄─────┘
```

| Step | Human | AI |
|---|---|---|
| A. Write | May write rules directly (English or Russian) | May draft rules; every rule carries an `Origin`; never approves |
| B1. Review spec | **Owns** the decision; answers "is this what the system *should* do?" | May pre-review (completeness, contradictions) and produce findings |
| Approve | Sets `Status: Approved` — **only a human** | — |
| C. Generate | — | Generates tests from approved rules, *blind* (see §C1) |
| B2. Review tests | Light check: each test matches its rule | May cross-check rule ↔ test mapping |
| Red test | Decides whether the code or the spec is wrong | Reports a mismatch; **never edits the assertion** |

### Choosing the format

| The behavior is… | Format | Spec layout | Test file |
|---|---|---|---|
| A state machine or decision table: same action, many (state, input) → outcome combinations | **T** — case table | Markdown table, one row per case | `domain/tests/` |
| A use case with fakes, ordering of checks, several collaborators, multi-step | **U** — use-case rule | Heading + Given/When/Then | `application/tests/` |
| A universal invariant over a large input space (value objects, parsers) | **P** — property | Heading + For all / Then / Oracle | `domain/tests/` |

A decision table that grows branching logic, or a use case that is really a single (state, input) → outcome
table, is in the wrong format: move it.

### Rule IDs

`<PREFIX>-<NN>`: 2–4 uppercase letters, a dash, 2–3 digits (`LC-01`, `AZ-12`, `DS-103`).

- One prefix per spec file; prefixes are registered in [`README.md`](./README.md).
- IDs are **unique across all specs and never reused** — a removed rule becomes `Deprecated`, it does not disappear.
- IDs carry no meaning beyond identity; do not renumber when inserting rules.

### Status

| Status | Meaning | Tests |
|---|---|---|
| `Draft` | Written, not yet approved by a human | **Must not** be generated yet |
| `Approved` | A human reviewed and accepted the rule | Required; enforced by `domain/tests/spec_traceability.rs` |
| `Deprecated` | Rule no longer valid | Tests must be removed; the ID stays reserved |

Changing *Given / When / Then / Oracle* of an `Approved` rule puts it back to `Draft`.
Typo and wording fixes that do not change meaning do not.

---

## A. Writing specs

### A1. Spec file layout

Use [`TEMPLATE.md`](./TEMPLATE.md). Files live in `docs/specs/domain/` or `docs/specs/application/`
(the traceability check only reads these two directories). Sections, in order:

1. Header — prefix, layer, unit under test (public path), format, planned test file.
2. **Vocabulary** — maps words used in rules to public API (states, helper names, fixtures). Rules may use only vocabulary terms.
3. **Rules** — the actual rules.
4. **Open Questions** — behavior the author could not decide; every rule that depends on one stays `Draft`.
5. **Findings** — contradictions or smells noticed while writing (docs vs code vs tests). Informational.

### A2. Rule anatomy

| Field | Required | Content |
|---|---|---|
| ID | yes | See above |
| Status | yes | `Draft` when written |
| Origin | yes | `human` — the author states the intended behavior · `adr:NNN` / `doc:<path>` — a written decision or public doc comment · `observed` — **an AI inferred it from code or existing tests; the intent is unconfirmed** |
| Given | yes | Initial state, using vocabulary terms and concrete values |
| When | yes | **One** action: a public function call with concrete arguments |
| Then | yes | Observable outcome: returned value, error *kind*, or state readable through the public API |
| Notes | no | Why the rule exists; link to the ADR |

For format **T**, the table columns are `ID | Status | Origin | <given columns…> | <then columns…>`;
the first two columns are parsed by the traceability check, so keep their order.
For format **P**, replace Given/When with **For all** (input domain) and add **Oracle**: how the
expected result is computed *independently of the implementation*.

### A3. Quality criteria (every rule must satisfy all)

| # | Criterion | Violation example |
|---|---|---|
| Q1 | **Atomic** — one behavior, one reason to fail | "publish increments revision and updates audit and rejects drafts" |
| Q2 | **Observable** — Then uses only what a public caller can see | "the private `touch` is called" |
| Q3 | **Concrete** — values, not adjectives | "a long title is rejected" → "a title of 101 chars is rejected, 100 accepted" |
| Q4 | **Closed vocabulary** — terms from the Vocabulary section, error kinds as enum variants | "fails with an error", "returns something sensible" |
| Q5 | **Intent, not implementation** — describes what the system should do, in words that survive a refactor | "calls `self.touch()` before `save`" |
| Q6 | **Complete state** — every state that matters is named (e.g. which publication state, which role set) | "when the document is published" with no revision |
| Q7 | **Boundaries explicit** — min, max, min−1, max+1, empty, one | rules only for the "typical" value |
| Q8 | **Negative cases present** — each guard has a rule that violates it | only happy paths |
| Q9 | **No duplicates, no contradictions** — see B1 | the same behavior stated twice with different outcomes |
| Q10 | **Not enforced by the compiler** — do not spec what the type system already guarantees | "`new` returns an `AttributeId`" |

### A4. For humans

- Start from the question *"what should happen?"*, not from the code. Write the sentence first; fill the template second.
- Russian is fine for Notes and Open Questions. Keep Given/When/Then terse and in the vocabulary of the spec; identifiers stay English.
- If you do not know the answer, write an **Open Question** instead of guessing. A rule with an unresolved question stays `Draft`.
- Prefer a table (T) when you catch yourself writing the same sentence with different values.
- Approve by changing `Status` to `Approved`. If the rule was `Origin: observed`, change `Origin` to `human` *only if* you actually decided that this is the desired behavior.

### A5. For AI

Allowed sources when **drafting** specs: public doc comments, `docs/**` and ADRs, public signatures, the
current tests (as *evidence of intent*, not as truth), and the implementation (to discover edge cases).

Mandatory behavior:

1. Every rule gets an `Origin`. A behavior found only in code or in existing tests is `observed`; never present it as `human`.
2. Never set `Status: Approved`. Write `Draft`.
3. Look for **contradictions** between doc comments, `docs/`, ADRs, existing tests and code. Record each in **Findings** with file and line, and do **not** pick a side: write an **Open Question** and leave dependent rules `Draft`.
4. Do not invent requirements. A rule that no source supports is not allowed; a *question* is.
5. Cover each public function: happy path, each documented error kind, each boundary, each guard.
6. Do not copy implementation structure into the vocabulary (no private fields, no helper functions that exist only in the implementation).
7. Report at the end: rules written (by Origin), open questions, findings.

---

## B. Reviewing

### B1. Reviewing a spec (before approval)

Human decides; an AI may produce the findings table first. Review in this order:

| # | Check | Question |
|---|---|---|
| 1 | **Intent** | Is each rule what the system *should* do? Reject rules that merely describe what it does. For every `Origin: observed` rule: is this desired? |
| 2 | **Completeness** | Does every public function have rules for happy path, every error kind it can return, and its boundaries? Is every cell of a state × action matrix covered or consciously excluded? |
| 3 | **Contradictions** | Between rules of this spec · with other specs (same unit) · with doc comments, `docs/api.md`, ADRs · with existing tests. Each contradiction is resolved by the human *before* approval. |
| 4 | **Precision** | Q1–Q10 from A3. Could two engineers write different tests from the same rule? |
| 5 | **Format fit** | T for tables, U for flows, P for invariants (see §0). |
| 6 | **Open Questions** | Are all answered or consciously deferred? Rules that depend on a deferred question stay `Draft`. |
| 7 | **IDs** | Unique, correct prefix, none reused. `cargo test -p domain --test spec_traceability` passes. |

**AI reviewer output** — a table, nothing else is edited:

| ID | Severity | Check # | Finding | Suggested resolution |
|---|---|---|---|---|

Severity: `blocker` (wrong or contradictory), `major` (incomplete or imprecise), `minor` (wording).
The AI reviewer does not change `Status` and does not edit rules.

### B2. Reviewing generated tests (after generation)

| # | Check | How |
|---|---|---|
| 1 | **1:1 mapping** | Each approved rule has exactly one test case (T) / test (U, P); each test cites one ID. The traceability check enforces presence, not correctness. |
| 2 | **Arrange = Given** | The setup establishes exactly the Given state, nothing more. |
| 3 | **Act = When** | One call. |
| 4 | **Assert = Then** | Asserts exactly what the rule says. No extra assertions that encode unspecified behavior; no missing ones. |
| 5 | **Independent expectations** | Expected values are literals or come from the spec, never computed by the code under test or by re-implementing it. |
| 6 | **Error kinds** | Matched by enum variant (`matches!`), not by message text — unless the message is part of the rule. |
| 7 | **Failure message** | Names the rule ID. |
| 8 | **No leaks** | No access to private details (impossible in `tests/` — if a test needs it, the spec or the public API is missing something). |
| 9 | **Determinism** | No sleeps, no real clock assertions (use ranges), no randomness outside `proptest`, no shared mutable state. |
| 10 | **Mismatch handling** | A failing test is reported as a *spec–code mismatch* (see C4), not "fixed". |

---

## C. Generating tests

### C1. Context — what the generator may see

| Allowed | Not allowed |
|---|---|
| The **approved** rules of the spec being implemented (ignore `Draft` and `Deprecated`) | Bodies of the functions under test |
| Public signatures and doc comments (`cargo doc --no-deps`, or only the `pub` items) | Existing inline tests of the unit under test |
| `domain::test_support`, `application::test_support` (builders, fixtures, fakes) | `git log` / `git blame` / diffs of the unit under test |
| `Cargo.toml` files, this document, [`examples.md`](./examples.md) | Other specs' tests as templates for *expected values* |

**Mechanism.** Tools that can only read whole files cannot hide function bodies. When an AI agent has
subagents, run generation in a **fresh subagent** whose prompt contains the allowed context and the explicit
prohibition above (see the skill). When it cannot, the agent states in its report that blindness could not be
enforced, and the human reviews with extra care.

If the public API or `test_support` lacks something needed to express a rule, **stop and report it** as a
finding (missing builder, missing constructor, unobservable outcome). Do not reach into private details and
do not widen visibility to make a test pass.

### C2. Procedure

1. Read the spec header, Vocabulary and the **Approved** rules only.
2. Create or extend the planned test file (name: `<prefix lowercase>_<topic>.rs`, e.g. `domain/tests/lc_document_lifecycle.rs`). First line: `//! Spec: docs/specs/<layer>/<file>.md — generated from approved rules; expectations come from the spec.`
3. Translate each rule 1:1 using the template for its format (C3).
4. `cargo test -p <crate> --test <file>`; then `cargo clippy --workspace --all-targets -- -D warnings`.
5. On red → C4. On green → run B2 yourself, then report.
6. Never edit an approved rule or a failing assertion to make a test pass.

### C3. Conventions per format

**Common**

- Test names and case ids contain the rule ID: table `id: "LC-03"`, tests `fn ds_05_publish_denied_without_permission()`, properties `fn ap_01_...`.
- Failure messages name the rule ID.
- Use `.expect("what was expected")` for setup. Never a bare `.unwrap()`; use `matches!` or `assert_eq!` — never string-match error messages.
- Time: domain functions take `now` as a parameter — pass a fixed `DateTime<Utc>`. Application services call `Utc::now()` internally — assert timestamps as ranges `before <= t && t <= after`.
- A `&'static DocumentTypeId` / `&'static SystemContext` may be obtained with `Box::leak` (the existing tests do this).

**T — case table** (`domain/tests/`)

- Closed enums for `Start` (given), `Action` (when), `Expect` (then). No strings as commands.
- `struct Case { id: &'static str, … }`; cases are data, the runner is ~15 lines.
- One `#[test]` per table; the runner executes **all** cases and fails once, listing **every** failing id.
- One action per case. If you need a sequence, it is a use-case rule (U).

**U — use case** (`application/tests/`)

- `#[tokio::test]`, plain Rust, fakes from `application::test_support` (feature `test-support`, already a dev-dependency).
- Arrange (fakes + service) → Act (one service call) → Assert (result variant, then persisted state through the fake).
- Assert the **error variant** of `ApplicationError` / `DomainError` with `matches!`.
- Assert persisted state by reading it back through the repository port, not by inspecting fake internals, unless the rule is about the fake's contract.

**P — property** (`domain/tests/`)

- `proptest!` with the generator named in **For all**; `ProptestConfig::with_cases(256)` unless the rule says otherwise.
- The **Oracle** is a separate, simple predicate or literal examples written in the test — never a copy of the production regex or logic.
- Also include the rule's *named examples* (boundaries) as ordinary asserts: properties find the unexpected, examples pin the boundary.
- Commit `proptest-regressions/` files; a shrunk failing input is a finding (C4).

### C4. Red-test protocol (spec–code mismatch)

When a generated test fails:

1. Re-read the rule and the test. If the **test** mis-translates the rule, fix the *test* (it is a generation error) and say so in the report.
2. Otherwise do **not** touch the assertion, the rule, or the code. Produce this report and stop on that rule:

```
SPEC–CODE MISMATCH
Rule:      <ID> — <title>        (spec file: <path>)
Expected:  <Then, quoted from the spec>
Actual:    <what the test observed — value / error kind>
Test:      <file>::<test name> [case id]
Hypothesis (optional, clearly labelled): <which side is probably wrong and why>
Decision needed from: human
```

3. Leave the failing test in place (mark nothing `#[ignore]` unless the human says so), and list the mismatch in the final report.

The human resolves it by changing the **code**, or by changing the **spec** (which returns the rule to `Draft` and
requires approval again).

### C5. When a spec changes

| Change | Action |
|---|---|
| Rule meaning changed | `Status → Draft`, human re-approves, tests regenerated |
| Rule removed | `Status → Deprecated`, delete its tests (the traceability check fails if a test references a deprecated rule) |
| Rule added | New ID, `Draft`, normal flow |
| Typo / wording only | Edit in place; tests untouched |
| Public API changed | Review the Vocabulary section first; then affected rules |

Existing inline tests stay until the area is covered by approved specs; then delete the inline tests that
duplicate an approved rule (keep inline tests of private invariants).
