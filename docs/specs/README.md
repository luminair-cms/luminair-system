# Behavior Specifications Index

This directory holds the declarative behavior specifications that serve as the single source of truth for AI-generated and human-reviewed tests (per [ADR-001](../adr/ADR-001-declarative-test-specifications.md)).

- **Guidelines**: [`GUIDELINES.md`](./GUIDELINES.md)
- **Specification Template**: [`TEMPLATE.md`](./TEMPLATE.md)
- **Implementation Examples**: [`examples.md`](./examples.md)

---

## Prefix Registry

Prefixes must be 2–4 uppercase letters, globally unique across all specifications, and never reused.

| Prefix | Domain / Component | Layer | Format | Spec Document |
|---|---|---|---|---|
| `LC` | Document Instance Lifecycle | `domain` | `T` (table) | [`domain/document-lifecycle.md`](domain/document-lifecycle.md) |
| `AZ` | Authorization Precedence & RBAC | `domain` | `T` (table) | [`domain/authorization-service.md`](domain/authorization-service.md) |
| `VO` | Value Object Invariants | `domain` | `P` (property) | [`domain/value-objects.md`](domain/value-objects.md) |
| `DS` | Documents Service Use Cases | `application` | `U` (use case) | [`application/documents-service.md`](application/documents-service.md) |
| `AR` | Access Requests & Enrollment | `application` | `U` (use case) | [`application/access-requests-service.md`](application/access-requests-service.md) |

---

## Traceability & Verification

The suite includes an automated gate:
```bash
cargo test -p domain --test spec_traceability
```
This checks that:
1. All specification files have well-formed rule IDs adhering to registered prefixes.
2. Every rule marked `Status: Approved` is referenced by an integration test in `<crate>/tests/`.
3. Every test referencing a rule maps to an active rule in the specifications.
