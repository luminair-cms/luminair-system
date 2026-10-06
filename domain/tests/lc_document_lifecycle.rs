//! Spec: docs/specs/domain/document-lifecycle.md — generated from behavior specification rules.

use chrono::Utc;
use domain::content::{DocumentInstance, PublicationState};
use domain::errors::DomainError;
use domain::test_support::{fixture_document_instance, test_user_id};

#[derive(Clone, Copy)]
enum StartState {
    DraftNone,
    Published(u32),
    DraftSome(u32),
}

#[derive(Clone, Copy)]
enum Action {
    Publish,
    Unpublish,
}

#[derive(Clone, Debug)]
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
    Case {
        id: "LC-01",
        start: StartState::DraftNone,
        action: Action::Publish,
        expect: Expect::PublishOk(1),
    },
    Case {
        id: "LC-02",
        start: StartState::Published(1),
        action: Action::Publish,
        expect: Expect::PublishOk(2),
    },
    Case {
        id: "LC-03",
        start: StartState::Published(1),
        action: Action::Unpublish,
        expect: Expect::UnpublishOk,
    },
    Case {
        id: "LC-04",
        start: StartState::DraftSome(1),
        action: Action::Publish,
        expect: Expect::PublishOk(2),
    },
    Case {
        id: "LC-05",
        start: StartState::DraftNone,
        action: Action::Unpublish,
        expect: Expect::ErrInvalidTransition,
    },
    Case {
        id: "LC-06",
        start: StartState::DraftSome(1),
        action: Action::Unpublish,
        expect: Expect::ErrInvalidTransition,
    },
];

#[test]
fn test_document_lifecycle_cases() {
    let now = Utc::now();
    let mut failures = Vec::new();

    for case in CASES {
        let mut inst = fixture_document_instance("article", Some("alice"));
        inst.audit.version = 1;

        match case.start {
            StartState::DraftNone => {
                inst.content.publication_state = PublicationState::Draft {
                    last_published_revision: None,
                };
            }
            StartState::Published(rev) => {
                inst.content.publication_state = PublicationState::Published {
                    revision: rev,
                    published_at: now,
                    published_by: Some(test_user_id("alice")),
                };
                inst.audit.version = 2;
            }
            StartState::DraftSome(rev) => {
                inst.content.publication_state = PublicationState::Draft {
                    last_published_revision: Some(rev),
                };
                inst.audit.version = 3;
            }
        }

        let user = Some(test_user_id("bob"));
        let res = match case.action {
            Action::Publish => inst.publish(user, now).map(Expect::PublishOk),
            Action::Unpublish => inst.unpublish(user, now).map(|_| Expect::UnpublishOk),
        };

        let passed = match (&case.expect, &res) {
            (Expect::PublishOk(exp_rev), Ok(Expect::PublishOk(act_rev))) => exp_rev == act_rev,
            (Expect::UnpublishOk, Ok(Expect::UnpublishOk)) => true,
            (Expect::ErrInvalidTransition, Err(DomainError::InvalidStateTransition { .. })) => true,
            _ => false,
        };

        if !passed {
            failures.push(format!("{}: expected matching outcome, got {:?}", case.id, res));
        }
    }

    assert!(failures.is_empty(), "Failed cases:\n- {}", failures.join("\n- "));
}

#[test]
fn lc_07_new_instance_defaults() {
    let now = Utc::now();
    let alice = test_user_id("alice");
    let inst = DocumentInstance::new(domain::test_support::test_doc_type_id("article"), Some(alice.clone()), now);

    assert_eq!(inst.content.publication_state, PublicationState::Draft { last_published_revision: None });
    assert_eq!(inst.audit.version, 1);
    assert_eq!(inst.audit.created_by, Some(alice.clone()));
    assert_eq!(inst.audit.updated_by, Some(alice));
    assert_eq!(inst.audit.created_at, now);
    assert_eq!(inst.audit.updated_at, now);
}

#[test]
fn lc_08_touch_increments_version_and_updates_audit() {
    let now = Utc::now();
    let t1 = now + chrono::Duration::seconds(10);
    let mut inst = fixture_document_instance("article", Some("alice"));
    let initial_version = inst.audit.version;
    let bob = test_user_id("bob");

    inst.touch(Some(bob.clone()), t1);

    assert_eq!(inst.audit.version, initial_version + 1);
    assert_eq!(inst.audit.updated_by, Some(bob));
    assert_eq!(inst.audit.updated_at, t1);
}
