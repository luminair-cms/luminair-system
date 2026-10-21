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
| `Owner` | User matches `instance.audit.created_by` | `instance.is_owned_by(user_id)` (`instance.audit.created_by == ActorId::User(user_id.clone())`) |
| `NonOwner` | User differs from `instance.audit.created_by` | `!instance.is_owned_by(user_id)` |
| `NoInstance` | Operation evaluated without a specific target instance | `instance: None` |
| `NoRoles` | Caller has empty assigned role list | `roles: &[]` |
| `Role(Perm)` | Role containing permission `Perm` | `Role { permissions: vec![Perm], .. }` |
| `Wildcard(P)` | Document permission with `None` doc type | `Permission::ReadDocument(None)` |
| `Scoped(P, T)` | Document permission scoped to type `T` | `Permission::ReadDocument(Some(T))` |

---

## Rules

| ID | Status | Origin | Instance & Ownership | Action | Assigned Roles | Expected Result |
|---|---|---|---|---|---|---|
| `AZ-01` | `Approved` | `adr:002` | `Owner` | `ReadDocument(Some("article"))` | `NoRoles` | `true` (Owner rule) |
| `AZ-02` | `Approved` | `adr:002` | `Owner` | `UpdateDocument(Some("article"))` | `NoRoles` | `true` (Owner rule) |
| `AZ-03` | `Approved` | `adr:002` | `Owner` | `DeleteDocument(Some("article"))` | `NoRoles` | `true` (Owner rule) |
| `AZ-04` | `Approved` | `adr:002` | `Owner` | `PublishDocument(Some("article"))` | `NoRoles` | `false` (Publishing always requires RBAC) |
| `AZ-05` | `Approved` | `adr:002` | `NonOwner` | `ReadDocument(Some("article"))` | `NoRoles` | `false` (Default deny) |
| `AZ-06` | `Approved` | `adr:002` | `NonOwner` | `UpdateDocument(Some("article"))` | `NoRoles` | `false` (Default deny) |
| `AZ-07` | `Approved` | `adr:002` | `NonOwner` | `DeleteDocument(Some("article"))` | `NoRoles` | `false` (Default deny) |
| `AZ-08` | `Approved` | `adr:002` | `NonOwner` | `PublishDocument(Some("article"))` | `NoRoles` | `false` (Default deny) |
| `AZ-09` | `Approved` | `adr:002` | `NonOwner` | `ReadDocument(Some("article"))` | `Role(Scoped(Read, "article"))` | `true` (Exact RBAC match) |
| `AZ-10` | `Approved` | `adr:002` | `NonOwner` | `ReadDocument(Some("article"))` | `Role(Scoped(Read, "news"))` | `false` (Type mismatch) |
| `AZ-11` | `Approved` | `adr:002` | `NonOwner` | `ReadDocument(Some("article"))` | `Role(Wildcard(Read))` | `true` (Wildcard RBAC match) |
| `AZ-12` | `Approved` | `adr:002` | `Owner` | `PublishDocument(Some("article"))` | `Role(Scoped(Publish, "article"))` | `true` (RBAC grants publish) |
| `AZ-13` | `Approved` | `adr:002` | `NonOwner` | `PublishDocument(Some("article"))` | `Role(Wildcard(Publish))` | `true` (Wildcard RBAC grants publish) |
| `AZ-14` | `Approved` | `adr:002` | `NoInstance` | `CreateDocument(Some("article"))` | `Role(Scoped(Create, "article"))` | `true` (Creation permitted) |
| `AZ-15` | `Approved` | `adr:002` | `NoInstance` | `CreateDocument(Some("article"))` | `NoRoles` | `false` (Creation denied without role) |
| `AZ-16` | `Approved` | `adr:002` | `NoInstance` | `ManageSchema` | `Role(ManageSchema)` | `true` (Administrative permission) |
| `AZ-17` | `Approved` | `adr:002` | `NoInstance` | `ManageRoles` | `Role(ManageUsers)` | `false` (Non-matching administrative permission) |
| `AZ-18` | `Approved` | `adr:002` | `NonOwner` | `DeleteDocument(Some("article"))` | `AdminRole` | `true` (Admin role covers all document actions) |
| `AZ-19` | `Approved` | `adr:004` | `Any / NoInstance` | `Any Permission` | `NoRoles (ActorId::System)` | `true` (System actor bypasses all checks) |
| `AZ-20` | `Approved` | `adr:004` | `Any` | `ReadDocument(Some("article"))` | `Role(Scoped(Read, "article")) (ActorId::Service)` | `true` (Service actor evaluates RBAC without owner rule) |

---

## Open Questions

1. **Q1 (Owner Create)**: Can a user without any assigned role create a document? (Current behavior: `CreateDocument` requires an explicit role; the owner rule applies only to *existing* instances with `created_by`). Confirmed.
2. **Q2 (Multi-role Evaluation)**: Roles are evaluated with short-circuit `any()`. If one role permits and another does not, permission is granted (union semantics). Confirmed.

---

## Findings

| # | Location | Code Says | Doc / Spec Says | Impact |
|---|---|---|---|---|
| 1 | `domain/src/auth/service.rs:8-13` | Doc comment historically claimed owner can publish | Code explicitly enforces that publish requires RBAC | Resolved in code before ADR-001 acceptance; doc comment updated |
| 2 | `domain/src/auth/service.rs` | Owner check checks `instance.is_owned_by(user_id)` | `instance.audit.created_by` is `ActorId`; system-created instances have `ActorId::System` (non-user, owner rule does not apply) | Desired behavior |
