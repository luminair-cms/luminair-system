# [PREFIX] — [Specification Title]

- **Prefix**: `XX`
- **Layer**: `domain` | `application`
- **Unit under test**: `path::to::TypeOrFunction`
- **Format**: `T` (case table) | `U` (use case) | `P` (property)
- **Test target**: `<crate>/tests/<prefix_lowercase>_<topic>.rs`

---

## Vocabulary

Map domain/system terms to concrete public API items, types, and values used in the rules.

| Term | Meaning / API symbol | Concrete representation |
|---|---|---|
| `Draft` | `PublicationState::Draft` | `last_published_revision: Option<u32>` |
| `Published` | `PublicationState::Published` | `revision: u32, published_at: DateTime<Utc>` |

---

## Rules

<!-- CHOOSE ONE FORMAT SECTION BELOW (DELETE THE OTHERS) -->

### Option Format T: Case Table (State Machines / Decision Tables)

| ID | Status | Origin | Given State | Action / Input | Expected Result / Next State |
|---|---|---|---|---|---|
| `XX-01` | `Draft` | `observed` | Initial condition | Method call & args | Expected outcome / `Err(DomainError::Variant)` |

---

### Option Format U: Use-Case Rules (Application Services)

### XX-01 — Brief Rule Description
- **Status**: `Draft`
- **Origin**: `human` | `adr:NNN` | `observed`
- **Given**: Preconditions, repository state, caller context permissions.
- **When**: Single use-case service invocation: `service.method(&caller, command).await`.
- **Then**: Observable result: `Ok(DocumentInstance)` with attributes, or `Err(ApplicationError::Unauthorized { .. })`, and verified persisted state.
- **Notes**: Context, rationale, or links to ADRs.

---

### Option Format P: Property Invariants (Value Objects)

### XX-01 — Invariant Name
- **Status**: `Draft`
- **Origin**: `human` | `adr:NNN` | `observed`
- **For all**: Input domain generator (e.g. any string matching regex pattern, any valid BCP47 code).
- **Then**: `Type::try_new(input)` returns `Ok(value)` where `value.as_ref() == expected`.
- **Oracle**: Independent validation rule or reference check.
- **Boundary Examples**: Concrete literal cases that must succeed or fail.

---

## Open Questions

Uncertainties or unverified behaviors discovered while writing the specification. Any rule depending on an unresolved question must remain `Status: Draft`.

1. **Q1**: Description of ambiguity.

---

## Findings

Discrepancies identified between docstrings, comments, architecture documents, ADRs, existing tests, and current code.

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `file.rs:123` | ... | ... | ... |
