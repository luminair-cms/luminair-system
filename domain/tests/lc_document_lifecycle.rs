//! Spec: docs/specs/domain/document-lifecycle.md — generated from behavior specification rules.

use std::collections::HashMap;

use chrono::Utc;
use domain::auth::ActorId;
use domain::content::header::{DocumentSnapshot, SnapshotId, SnapshotReason};
use domain::content::{DocumentInstance, PublicationState};
use domain::errors::DomainError;
use domain::test_support::{fixture_document_instance, test_user_id};
use uuid::Uuid;

#[derive(Clone, Copy)]
enum StartState {
    DraftNone,
    Published(u32),
    Unpublished(u32),
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
        start: StartState::Unpublished(1),
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
        start: StartState::Unpublished(1),
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
                inst.content.publication_state = PublicationState::Draft;
            }
            StartState::Published(rev) => {
                inst.content.publication_state = PublicationState::Published {
                    revision: rev,
                    published_at: now,
                    published_by: ActorId::User(test_user_id("alice")),
                };
                inst.audit.version = 2;
            }
            StartState::Unpublished(rev) => {
                inst.content.publication_state = PublicationState::Unpublished {
                    last_published_revision: rev,
                    unpublished_at: now,
                    unpublished_by: ActorId::User(test_user_id("alice")),
                };
                inst.audit.version = 3;
            }
        }

        let user = ActorId::User(test_user_id("bob"));
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
            failures.push(format!(
                "{}: expected matching outcome, got {:?}",
                case.id, res
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Failed cases:\n- {}",
        failures.join("\n- ")
    );
}

#[test]
fn lc_07_new_instance_defaults() {
    let now = Utc::now();
    let alice = test_user_id("alice");
    let inst = DocumentInstance::new(
        domain::test_support::test_doc_type_id("article"),
        alice.clone(),
        now,
    );

    assert_eq!(inst.content.publication_state, PublicationState::Draft);
    assert_eq!(inst.audit.version, 1);
    assert_eq!(inst.audit.created_by, ActorId::User(alice.clone()));
    assert_eq!(inst.audit.updated_by, ActorId::User(alice));
    assert_eq!(inst.audit.created_at, now);
    assert_eq!(inst.audit.updated_at, now);
}

#[test]
fn lc_13_restore_snapshot_on_draft_fails() {
    let now = Utc::now();
    let alice = test_user_id("alice");
    let mut inst = fixture_document_instance("article", Some("alice"));
    assert_eq!(inst.audit.version, 1);

    let snap = DocumentSnapshot {
        id: SnapshotId::new(Uuid::now_v7()),
        document_type_id: inst.document_type_id.clone(),
        document_id: inst.id,
        revision: 1,
        fields: HashMap::new(),
        relations: HashMap::new(),
        created_at: now,
        created_by: ActorId::User(alice.clone()),
        reason: SnapshotReason::Publish,
    };

    let res = inst.restore_snapshot(&snap, alice, now);
    assert!(matches!(
        res,
        Err(DomainError::InvalidStateTransition { .. })
    ));
    assert_eq!(inst.audit.version, 1);
    assert_eq!(inst.content.publication_state, PublicationState::Draft);
}

#[test]
fn lc_08_touch_increments_version_and_updates_audit() {
    let now = Utc::now();
    let t1 = now + chrono::Duration::seconds(10);
    let mut inst = fixture_document_instance("article", Some("alice"));
    let initial_version = inst.audit.version;
    let bob = test_user_id("bob");

    inst.touch(bob.clone(), t1);

    assert_eq!(inst.audit.version, initial_version + 1);
    assert_eq!(inst.audit.updated_by, ActorId::User(bob));
    assert_eq!(inst.audit.updated_at, t1);
}

#[test]
fn lc_09_edit_draft_transitions_to_modified() {
    let now = Utc::now();
    let t1 = now + chrono::Duration::seconds(10);
    let t2 = now + chrono::Duration::seconds(20);
    let mut inst = fixture_document_instance("article", Some("alice"));
    let alice = test_user_id("alice");

    inst.publish(alice.clone(), t1).unwrap();
    assert_eq!(inst.audit.version, 2);

    inst.edit_draft(alice.clone(), t2);
    assert_eq!(inst.audit.version, 3);
    assert_eq!(inst.audit.updated_by, ActorId::User(alice.clone()));
    assert_eq!(inst.audit.updated_at, t2);

    assert!(matches!(
        inst.content.publication_state,
        PublicationState::Modified {
            live_revision: 1,
            draft_version: 3,
            ..
        }
    ));
}

#[test]
fn lc_10_discard_draft_restores_published() {
    let now = Utc::now();
    let t1 = now + chrono::Duration::seconds(10);
    let t2 = now + chrono::Duration::seconds(20);
    let mut inst = fixture_document_instance("article", Some("alice"));
    let alice = test_user_id("alice");

    inst.publish(alice.clone(), t1).unwrap();
    inst.edit_draft(alice, t2);
    assert_eq!(inst.audit.version, 3);

    inst.discard_draft().unwrap();
    assert_eq!(inst.audit.version, 3);
    assert!(matches!(
        inst.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));
}

#[test]
fn lc_11_discard_draft_on_draft_fails() {
    let mut inst = fixture_document_instance("article", Some("alice"));
    assert_eq!(inst.audit.version, 1);

    let res = inst.discard_draft();
    assert!(matches!(
        res,
        Err(DomainError::InvalidStateTransition { .. })
    ));
    assert_eq!(inst.audit.version, 1);
}

#[test]
fn lc_12_restore_snapshot_transitions_to_modified() {
    let now = Utc::now();
    let t1 = now + chrono::Duration::seconds(10);
    let t2 = now + chrono::Duration::seconds(20);
    let t3 = now + chrono::Duration::seconds(30);
    let mut inst = fixture_document_instance("article", Some("alice"));
    let alice = test_user_id("alice");

    // Rev 1
    inst.publish(alice.clone(), t1).unwrap();
    // Rev 2
    inst.publish(alice.clone(), t2).unwrap();
    assert_eq!(inst.audit.version, 3);

    // Snapshot of Rev 1
    let snap = DocumentSnapshot {
        id: SnapshotId::new(Uuid::now_v7()),
        document_type_id: inst.document_type_id.clone(),
        document_id: inst.id,
        revision: 1,
        fields: HashMap::new(),
        relations: HashMap::new(),
        created_at: t1,
        created_by: ActorId::User(alice.clone()),
        reason: SnapshotReason::Publish,
    };

    inst.restore_snapshot(&snap, alice.clone(), t3).unwrap();
    assert_eq!(inst.audit.version, 4);
    assert_eq!(inst.audit.updated_by, ActorId::User(alice));
    assert_eq!(inst.audit.updated_at, t3);

    assert!(matches!(
        inst.content.publication_state,
        PublicationState::Modified {
            live_revision: 2,
            draft_version: 4,
            ..
        }
    ));
}
