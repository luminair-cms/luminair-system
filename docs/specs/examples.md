# Behavior Specifications — Worked Examples

This document demonstrates the three specification formats described in [`GUIDELINES.md`](./GUIDELINES.md) and shows how each format translates directly into idiomatic, maintainable Rust test code.

---

## Example 1: Format T — Case Table (State Machines & Decision Matrices)

### The Specification (`docs/specs/domain/document-lifecycle.md`)

```markdown
# LC — Document Instance Lifecycle

- **Prefix**: `LC`
- **Layer**: `domain`
- **Unit under test**: `domain::content::DocumentInstance`
- **Format**: `T` (case table)
- **Test target**: `domain/tests/lc_document_lifecycle.rs`

## Vocabulary
- `Draft(None)`: Draft with no previous publication (`last_published_revision: None`).
- `Draft(Some(N))`: Draft with last published revision `N`.
- `Published(N)`: Published at revision `N`.
- `Publish`: calling `instance.publish(user, now)`.
- `Unpublish`: calling `instance.unpublish(user, now)`.

## Rules
| ID | Status | Origin | Given State | Action | Expected Result | Next State |
|---|---|---|---|---|---|---|
| `LC-01` | `Approved` | `adr:004` | `Draft(None)` | `Publish` | `Ok(1)` | `Published(1)` |
| `LC-02` | `Approved` | `adr:004` | `Published(1)` | `Publish` | `Ok(2)` | `Published(2)` |
| `LC-03` | `Approved` | `adr:004` | `Published(2)` | `Unpublish` | `Ok(())` | `Draft(Some(2))` |
| `LC-04` | `Approved` | `adr:004` | `Draft(Some(2))` | `Publish` | `Ok(3)` | `Published(3)` |
| `LC-05` | `Approved` | `adr:004` | `Draft(None)` | `Unpublish` | `Err(InvalidStateTransition)` | `Draft(None)` |
| `LC-06` | `Approved` | `adr:004` | `Draft(Some(2))` | `Unpublish` | `Err(InvalidStateTransition)` | `Draft(Some(2))` |
```

### The Rust Test Implementation (`domain/tests/lc_document_lifecycle.rs`)

```rust
//! Spec: docs/specs/domain/document-lifecycle.md — generated from approved rules.

use chrono::Utc;
use domain::content::{DocumentInstance, PublicationState};
use domain::errors::DomainError;
use domain::test_support::fixture_document_instance;

#[derive(Clone, Copy)]
enum StartState {
    DraftNone,
    DraftSome(u32),
    Published(u32),
}

#[derive(Clone, Copy)]
enum Action {
    Publish,
    Unpublish,
}

#[derive(Clone)]
enum Expect {
    PublishOk(u32),
    UnpublishOk,
    ErrInvalidTransition,
}

struct Case {
    id: &'static str,
    start: StartState,
    action: Action,
    expect: Expect,
}

const CASES: &[Case] = &[
    Case { id: "LC-01", start: StartState::DraftNone, action: Action::Publish, expect: Expect::PublishOk(1) },
    Case { id: "LC-02", start: StartState::Published(1), action: Action::Publish, expect: Expect::PublishOk(2) },
    Case { id: "LC-03", start: StartState::Published(2), action: Action::Unpublish, expect: Expect::UnpublishOk },
    Case { id: "LC-04", start: StartState::DraftSome(2), action: Action::Publish, expect: Expect::PublishOk(3) },
    Case { id: "LC-05", start: StartState::DraftNone, action: Action::Unpublish, expect: Expect::ErrInvalidTransition },
    Case { id: "LC-06", start: StartState::DraftSome(2), action: Action::Unpublish, expect: Expect::ErrInvalidTransition },
];

#[test]
fn test_document_lifecycle_cases() {
    let now = Utc::now();
    let mut failures = Vec::new();

    for case in CASES {
        let mut inst = fixture_document_instance("article", Some("alice"));
        match case.start {
            StartState::DraftNone => inst.content.publication_state = PublicationState::Draft { last_published_revision: None },
            StartState::DraftSome(r) => inst.content.publication_state = PublicationState::Draft { last_published_revision: Some(r) },
            StartState::Published(r) => inst.content.publication_state = PublicationState::Published { revision: r, published_at: now, published_by: None },
        }

        let res = match case.action {
            Action::Publish => inst.publish(None, now).map(|rev| Expect::PublishOk(rev)),
            Action::Unpublish => inst.unpublish(None, now).map(|_| Expect::UnpublishOk),
        };

        let matched = match (&case.expect, res) {
            (Expect::PublishOk(exp), Ok(Expect::PublishOk(act))) => *exp == act,
            (Expect::UnpublishOk, Ok(Expect::UnpublishOk)) => true,
            (Expect::ErrInvalidTransition, Err(DomainError::InvalidStateTransition { .. })) => true,
            _ => false,
        };

        if !matched {
            failures.push(case.id);
        }
    }

    assert!(failures.is_empty(), "Failed cases: {:?}", failures);
}
```

---

## Example 2: Format U — Use Case Rules (Application Layer)

### The Specification (`docs/specs/application/documents-service.md`)

```markdown
# DS — Documents Application Service

- **Prefix**: `DS`
- **Layer**: `application`
- **Unit under test**: `application::services::DocumentsService`
- **Format**: `U` (use case)
- **Test target**: `application/tests/ds_documents_service.rs`

### DS-01 — Publish Requires Draft-and-Publish Enabled
- **Status**: `Approved`
- **Origin**: `adr:004`
- **Given**: A document type with `draft_and_publish: false` and an existing document instance.
- **When**: Caller with `PublishDocument` permission invokes `service.publish(caller, PublishDocumentCommand::new(type_id, instance_id)).await`.
- **Then**: Returns `Err(ApplicationError::Conflict(msg))` containing `"does not support draft-and-publish"`.

### DS-02 — Non-Owner Denied Without Explicit Role
- **Status**: `Approved`
- **Origin**: `adr:002`
- **Given**: A document created by `owner_user`, and a caller `stranger_user` with no roles.
- **When**: Stranger invokes `service.update(caller, UpdateDocumentCommand::new(type_id, instance_id, fields)).await`.
- **Then**: Returns `Err(ApplicationError::Unauthorized { .. })`.
```

### The Rust Test Implementation (`application/tests/ds_documents_service.rs`)

```rust
//! Spec: docs/specs/application/documents-service.md — generated from approved rules.

use std::collections::HashMap;
use application::commands::documents::{PublishDocumentCommand, UpdateDocumentCommand};
use application::context::CallerContext;
use application::errors::ApplicationError;
use application::services::{DocumentsService, DocumentsServiceImpl};
use application::test_support::FakeDocumentInstanceRepository;
use domain::auth::Permission;
use domain::content::DocumentInstance;
use domain::schema::{DocumentType, DocumentTypeBuilder, SchemaRegistry};
use domain::system::SystemContext;
use domain::test_support::{fixture_system_config, text_field};

fn setup_test_context(draft_and_publish: bool) -> (&'static SystemContext, &'static DocumentType) {
    let mut builder = DocumentTypeBuilder::collection("article");
    builder = builder.with_title("Article").with_field(text_field("title"));
    if !draft_and_publish {
        builder = builder.without_draft_and_publish();
    }
    let dt = builder.build();
    let type_id = dt.id.clone();
    let schema = SchemaRegistry::new(vec![dt]);
    let ctx: &'static SystemContext = Box::leak(Box::new(SystemContext::new(schema, fixture_system_config())));
    let leaked_dt = ctx.find_type(&type_id).unwrap();
    (ctx, leaked_dt)
}

#[tokio::test]
async fn ds_01_publish_requires_draft_and_publish_enabled() {
    let (ctx, doc_type) = setup_test_context(false);
    let repo = FakeDocumentInstanceRepository::new();
    let inst = DocumentInstance::new(doc_type.id.clone(), None, chrono::Utc::now());
    repo.save(&inst).await.expect("seeded instance");
    let service = DocumentsServiceImpl::new(repo, ctx);
    let caller = CallerContext::system();

    let res = service.publish(&caller, PublishDocumentCommand::new(&doc_type.id, inst.id)).await;

    assert!(matches!(res, Err(ApplicationError::Conflict(_))), "expected Conflict, got {:?}", res);
}

#[tokio::test]
async fn ds_02_non_owner_denied_without_explicit_role() {
    let (ctx, doc_type) = setup_test_context(true);
    let repo = FakeDocumentInstanceRepository::new();
    let owner_id = domain::test_support::test_user_id("owner");
    let stranger_id = domain::test_support::test_user_id("stranger");
    
    let inst = DocumentInstance::new(doc_type.id.clone(), Some(owner_id), chrono::Utc::now());
    repo.save(&inst).await.expect("seeded instance");
    let service = DocumentsServiceImpl::new(repo, ctx);
    let stranger_caller = CallerContext::new(stranger_id, vec![]);

    let res = service.update(&stranger_caller, UpdateDocumentCommand::new(&doc_type.id, inst.id, HashMap::new())).await;

    assert!(matches!(res, Err(ApplicationError::Unauthorized { .. })), "expected Unauthorized, got {:?}", res);
}
```

---

## Example 3: Format P — Property-Based Invariant (Value Objects)

### The Specification (`docs/specs/domain/value-objects.md`)

```markdown
# VO — Domain Value Objects

- **Prefix**: `VO`
- **Layer**: `domain`
- **Unit under test**: `domain::schema::AttributeId`
- **Format**: `P` (property)
- **Test target**: `domain/tests/vo_value_objects.rs`

### VO-01 — AttributeId Kebab-Case Acceptance
- **Status**: `Approved`
- **Origin**: `doc:domain::schema::AttributeId`
- **For all**: Strings of length 2..=64 generated by regex `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
- **Then**: `AttributeId::try_new(s)` succeeds with `Ok(attr)` where `attr.as_ref() == s`.
- **Oracle**: Direct match against specification grammar.
- **Boundary Examples**: `"ab"` (min 2 chars, valid), `"a-1"` (valid hyphenated), `"-a"` (invalid leading hyphen), `"A"` (invalid uppercase).
```

### The Rust Test Implementation (`domain/tests/vo_value_objects.rs`)

```rust
//! Spec: docs/specs/domain/value-objects.md — generated from approved rules.

use domain::schema::AttributeId;
use proptest::prelude::*;

proptest! {
    #[test]
    fn vo_01_attribute_id_valid_kebab_case(s in "[a-z][a-z0-9]{1,3}(-[a-z0-9]{1,4}){0,3}") {
        prop_assume!(s.chars().count() >= 2 && s.chars().count() <= 64);
        let res = AttributeId::try_new(&s);
        prop_assert!(res.is_ok(), "Expected valid AttributeId for '{}', got {:?}", s, res);
        prop_assert_eq!(res.unwrap().as_ref(), s.as_str());
    }
}

#[test]
fn vo_01_boundary_examples() {
    assert!(AttributeId::try_new("ab").is_ok());
    assert!(AttributeId::try_new("a-1").is_ok());
    assert!(AttributeId::try_new("-a").is_err());
    assert!(AttributeId::try_new("A").is_err());
    assert!(AttributeId::try_new("a").is_err()); // below min length 2
}
```
