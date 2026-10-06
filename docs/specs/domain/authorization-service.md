# AZ — Authorization & RBAC Evaluation Specification

- **Prefix**: `AZ`
- **Layer**: `domain`
- **Unit under test**: `domain::auth::AuthorizationService`
- **Format**: `T` (case table)
- **Test target**: `domain/tests/az_authorization_service.rs`

---

## Vocabulary

| Term | Meaning / API symbol | Concrete representation |
|---|---|---|
| `Owner` | User matches `instance.audit.created_by` | `user_id == instance.audit.created_by.unwrap()` |
| `NonOwner` | User differs from `instance.audit.created_by` | `user_id != instance.audit.created_by.unwrap()` |
| `NoInstance` | Operation evaluated without a specific target instance | `instance: None` |
| `NoRoles` | Caller has empty assigned role list | `roles: &[]` |
| `Role(Perm)` | Role containing permission `Perm` | `Role { permissions: vec![Perm], .. }` |
| `Wildcard(P)` | Document permission with `None` doc type | `Permission::ReadDocument(None)` |
| `Scoped(P, T)` | Document permission scoped to type `T` | `Permission::ReadDocument(Some(T))` |

---

## Rules

| ID | Status | Origin | Instance & Ownership | Action | Assigned Roles | Expected Result |
|---|---|---|---|---|---|---|
| `AZ-01` | `Draft` | `adr:002` | `Owner` | `ReadDocument(Some("article"))` | `NoRoles` | `true` (Owner rule) |
| `AZ-02` | `Draft` | `adr:002` | `Owner` | `UpdateDocument(Some("article"))` | `NoRoles` | `true` (Owner rule) |
| `AZ-03` | `Draft` | `adr:002` | `Owner` | `DeleteDocument(Some("article"))` | `NoRoles` | `true` (Owner rule) |
| `AZ-04` | `Draft` | `adr:002` | `Owner` | `PublishDocument(Some("article"))` | `NoRoles` | `false` (Publishing always requires RBAC) |
| `AZ-05` | `Draft` | `adr:002` | `NonOwner` | `ReadDocument(Some("article"))` | `NoRoles` | `false` (Default deny) |
| `AZ-06` | `Draft` | `adr:002` | `NonOwner` | `UpdateDocument(Some("article"))` | `NoRoles` | `false` (Default deny) |
| `AZ-07` | `Draft` | `adr:002` | `NonOwner` | `DeleteDocument(Some("article"))` | `NoRoles` | `false` (Default deny) |
| `AZ-08` | `Draft` | `adr:002` | `NonOwner` | `PublishDocument(Some("article"))` | `NoRoles` | `false` (Default deny) |
| `AZ-09` | `Draft` | `adr:002` | `NonOwner` | `ReadDocument(Some("article"))` | `Role(Scoped(Read, "article"))` | `true` (Exact RBAC match) |
| `AZ-10` | `Draft` | `adr:002` | `NonOwner` | `ReadDocument(Some("article"))` | `Role(Scoped(Read, "news"))` | `false` (Type mismatch) |
| `AZ-11` | `Draft` | `adr:002` | `NonOwner` | `ReadDocument(Some("article"))` | `Role(Wildcard(Read))` | `true` (Wildcard RBAC match) |
| `AZ-12` | `Draft` | `adr:002` | `Owner` | `PublishDocument(Some("article"))` | `Role(Scoped(Publish, "article"))` | `true` (RBAC grants publish) |
| `AZ-13` | `Draft` | `adr:002` | `NonOwner` | `PublishDocument(Some("article"))` | `Role(Wildcard(Publish))` | `true` (Wildcard RBAC grants publish) |
| `AZ-14` | `Draft` | `adr:002` | `NoInstance` | `CreateDocument(Some("article"))` | `Role(Scoped(Create, "article"))` | `true` (Creation permitted) |
| `AZ-15` | `Draft` | `adr:002` | `NoInstance` | `CreateDocument(Some("article"))` | `NoRoles` | `false` (Creation denied without role) |
| `AZ-16` | `Draft` | `adr:002` | `NoInstance` | `ManageSchema` | `Role(ManageSchema)` | `true` (Administrative permission) |
| `AZ-17` | `Draft` | `adr:002` | `NoInstance` | `ManageRoles` | `Role(ManageUsers)` | `false` (Non-matching administrative permission) |
| `AZ-18` | `Draft` | `adr:002` | `NonOwner` | `DeleteDocument(Some("article"))` | `AdminRole` | `true` (Admin role covers all document actions) |

---

## Open Questions

1. **Q1 (Owner Create)**: Can a user without any assigned role create a document? (Current behavior: `CreateDocument` requires an explicit role; the owner rule applies only to *existing* instances with `created_by`). Confirmed.
2. **Q2 (Multi-role Evaluation)**: Roles are evaluated with short-circuit `any()`. If one role permits and another does not, permission is granted (union semantics). Confirmed.

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `domain/src/auth/service.rs:8-13` | Doc comment historically claimed owner can publish | Code explicitly enforces that publish requires RBAC | Resolved in code before ADR-001 acceptance; doc comment updated |
| 2 | `domain/src/auth/service.rs:27-31` | Owner check only checks `is_owned_by(user_id)` | If `instance.audit.created_by` is `None` (system created), nobody is owner | Desired behavior |
