# Skill: Writing Tests for Luminair

Load this skill when asked to write or modify tests.
For the behavior specification approach, see [`ADR-001`](../../docs/adr/ADR-001-declarative-test-specifications.md) and [`.ai/skills/spec-testing.md`](spec-testing.md).
For operational guidelines, see [`docs/specs/GUIDELINES.md`](../../docs/specs/GUIDELINES.md).

## Test Organization & Locations

```
Kind of Test                     Location                      Access
────────────────────────────────────────────────────────────────────────────────
Spec-backed domain tests         domain/tests/*.rs             Public API only
Spec-backed use-case tests       application/tests/*.rs        Public API + test_support fakes
Internal invariant unit tests    <crate>/src/**/#[cfg(test)]   Module internals
Infrastructure integration tests infrastructure/tests/*.rs     Database & external systems
```

- **Spec-backed tests** (rules in `docs/specs/`) must live in `<crate>/tests/*.rs` as integration tests. They test external, observable behavior only.
- **Inline unit tests** (`#[cfg(test)]`) are reserved for private algorithms, helpers, or legacy tests not yet migrated.
- **Fake repository self-tests** live directly inside `application/src/test_support.rs`.

---

## Fake Repositories Pattern (Application Layer)

Use the built-in thread-safe fake repositories from `application::test_support`:
- `FakeDocumentInstanceRepository`: in-memory storage supporting filtering, deterministic pagination, and batch relation lookups.
- `FakeRoleRepository`: in-memory role definitions.
- `FakeUserRoleAssignmentRepository`: user-role mappings.
- `FakeAccessRequestRepository`: pending and resolved access requests.

Example use-case test setup:

```rust
use domain::schema::DocumentTypeId;
use domain::system::SystemContext;
use domain::test_support::fixture_system_config;
use application::context::CallerContext;
use application::services::{DocumentsService, DocumentsServiceImpl};
use application::test_support::FakeDocumentInstanceRepository;

#[tokio::test]
async fn test_create_document_success() {
    let repo = FakeDocumentInstanceRepository::new();
    let config = fixture_system_config();
    let schema = domain::schema::SchemaRegistry::new(vec![/* ... */]);
    let context: &'static SystemContext = Box::leak(Box::new(SystemContext::new(schema, config)));
    let service = DocumentsServiceImpl::new(repo, context);
    let caller = CallerContext::system();
    
    // ... arrange, act, assert
}
```

---

## Integration Test Skeleton (Infrastructure Layer)

```rust
// infrastructure/tests/migrations_test.rs or repository_test.rs
use sqlx::PgPool;

#[sqlx::test(migrations = "../migrations")]
async fn test_create_instance_persists_to_db(pool: PgPool) {
    // sqlx::test provisions a clean DB and runs migrations automatically
    // ... arrange, act, assert
}
```

---

## Naming & Traceability Conventions

- Spec-backed use cases: `fn <prefix_lowercase>_<num>_<brief_description>()`, e.g. `fn ds_01_create_draft_success()`
- Spec-backed table cases: field `id: "LC-01"` in `Case` struct
- Property tests: `fn <prefix_lowercase>_<num>_<invariant_name>()`, e.g. `fn vo_01_attribute_id_valid_kebab_case()`
- Legacy inline unit tests: `test_<method>_<scenario>`

---

## What NOT to Do

- **NEVER** edit assertions when a spec-backed test fails — report a spec–code mismatch (see `.ai/skills/spec-testing.md`).
- **NEVER** inspect private fields or implementation details in spec-backed tests.
- Do not call `.unwrap()` in tests — use `.expect("descriptive reason")` for setup or `?` with `Result`.
- Do not share global mutable state between tests (use isolated instances or `Box::leak` for immutable read-only contexts).
- Do not import `infrastructure` types or database pools in `domain` or `application` tests.
