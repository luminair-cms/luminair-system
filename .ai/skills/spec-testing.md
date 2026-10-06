# Skill: Specification-Driven Testing (ADR-001)

Load this skill whenever:
1. Drafting or reviewing behavioral specifications in `docs/specs/`.
2. Generating Rust test code from approved behavior specifications.
3. Reviewing test implementations against specifications.
4. Handling failing tests generated from specifications (spec–code mismatches).

Operational handbook: [`docs/specs/GUIDELINES.md`](../../docs/specs/GUIDELINES.md)
Format template: [`docs/specs/TEMPLATE.md`](../../docs/specs/TEMPLATE.md)
Reference examples: [`docs/specs/examples.md`](../../docs/specs/examples.md)

---

## 1. Core Workflow Roles & Rules

Per [`AGENTS.md`](../../AGENTS.md) §0 and [`ADR-001`](../../docs/adr/ADR-001-declarative-test-specifications.md):
- **Specs First**: Main code behavior is governed by written specification rules.
- **Human Owns Intent**: An AI agent may draft specs and suggest findings, but **ONLY a human can mark a rule as `Approved`**.
- **Blind Generation**: When generating tests from an approved spec, the AI must NOT read function bodies or inline tests of the unit under test. Only public signatures, `test_support`, and the spec rules are permitted.
- **Red is a Finding**: If a generated test fails against existing code, the AI must **NEVER** edit the assertion or the rule to make it pass. It must report a **SPEC–CODE MISMATCH** for human decision.

---

## 2. Choosing the Format

| Behavior Type | Target Format | Spec Location | Test Target |
|---|---|---|---|
| State machines, decision matrices, lifecycles | **Option 2 (Case Table)** | `docs/specs/domain/*.md` | `domain/tests/<prefix>_*.rs` |
| Application use cases, multi-step flows, authorization gates | **Option 3 (Given/When/Then)** | `docs/specs/application/*.md` | `application/tests/<prefix>_*.rs` |
| Value-object boundaries, regex patterns, invariant laws | **Option 4 (`proptest`)** | `docs/specs/domain/*.md` | `domain/tests/<prefix>_*.rs` |

---

## 3. Protocol: Drafting Specs (AI Agent)

When asked to write or propose behavior specs:
1. Check `docs/specs/README.md` to pick or register a unique prefix (`LC-`, `AZ-`, `DS-`, etc.).
2. Follow `docs/specs/TEMPLATE.md`.
3. Every rule must have:
   - `ID`: e.g. `LC-01`.
   - `Status`: Always `Draft` (never `Approved`).
   - `Origin`: `human` (if instructed by user), `adr:NNN` / `doc:<path>` (from docs), or `observed` (inferred from code/tests).
   - Concrete inputs, single action, and observable outcome (exact error enum variant or public state).
4. Check for contradictions between code, docstrings, ADRs, and tests. Log any discrepancies in the **Findings** section and formulate **Open Questions**.
5. Do not invent ungrounded business requirements.

---

## 4. Protocol: Reviewing Specs (AI Pre-Review)

When asked to review a spec before human approval:
1. Verify rule quality criteria Q1–Q10 in `docs/specs/GUIDELINES.md`:
   - Is each rule atomic and observable through public API only?
   - Are error kinds specific enum variants instead of vague strings?
   - Are boundary values explicit (min, max, empty, overflow)?
   - Are negative/denial cases present?
2. Format review output as a concise Markdown table:
   `| ID | Severity (blocker/major/minor) | Check | Finding | Suggested Resolution |`
3. Do not modify rule statuses or edit the spec text directly during review.

---

## 5. Protocol: Generating Tests (Blind Generation)

When directed to generate tests for **Approved** rules:
1. **Context Filtering**:
   - DO NOT inspect function implementation bodies or existing inline tests in `src/`.
   - Read ONLY:
     - The approved rules and vocabulary in `docs/specs/**/<file>.md`.
     - The public API (`pub` types, traits, methods) and docstrings.
     - `domain::test_support` / `application::test_support`.
2. **Implementation Target**:
   - Create or update the integration test file in `<crate>/tests/<prefix_lowercase>_<topic>.rs`.
   - Option 2: Table runner executing all cases in one test and reporting all failing IDs.
   - Option 3: Separate `#[tokio::test]` functions named `<prefix_lowercase>_<num>_<scenario>()`.
   - Option 4: `proptest!` macro blocks testing invariants across generated inputs.
3. **Traceability**:
   - Ensure the rule ID (`LC-01`) appears in the test name or table case `id`.
   - Verify with `cargo test -p domain --test spec_traceability`.

---

## 6. Protocol: Spec–Code Mismatch Reporting

If a generated test fails:
1. Verify if the test mis-translated the approved rule. If so, fix the test code.
2. If the test faithfully implements the approved rule and the code fails, **STOP IMMEDIATELY**.
3. Output the mismatch report:
```markdown
### SPEC–CODE MISMATCH DETECTED
- **Rule ID**: <PREFIX-NN> (<Title>)
- **Spec File**: docs/specs/...
- **Expected (from Spec)**: <Quote exact Then condition>
- **Actual (from Execution)**: <Observed return value or error variant>
- **Failing Test**: <test_file>::<test_function>
- **Analysis**: <Brief explanation of what the implementation did vs what the rule specified>
- **Awaiting Decision**: Human must decide whether the code needs fixing or the spec rule needs revision.
```
4. Never alter assertions to match unexpected code behavior.
