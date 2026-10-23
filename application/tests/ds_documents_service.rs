use std::collections::HashMap;

use application::commands::documents::*;
use application::context::CallerContext;
use application::errors::{ApplicationError, ConflictCode};
use application::services::{DocumentsService, DocumentsServiceImpl};
use application::test_support::FakeDocumentInstanceRepository;
use domain::content::{
    ContentValue, DocumentStatus, DomainValue, FieldFilter, Pagination, PrimitiveValue,
    PublicationState, SnapshotReason, UnpublishDraftResolution, VersionView,
};
use domain::errors::DomainError;
use domain::schema::{DocumentType, RelationDefinition, RelationType, SchemaRegistry};
use domain::system::SystemContext;
use domain::test_support::{
    DocumentTypeBuilder, fixture_system_config, test_attr_id, test_user_id, text_field,
};

fn setup_service_fixture(
    draft_and_publish: bool,
    single_type: bool,
) -> (
    DocumentsServiceImpl<FakeDocumentInstanceRepository>,
    &'static DocumentType,
    CallerContext,
) {
    let mut builder = if single_type {
        DocumentTypeBuilder::single("article")
    } else {
        DocumentTypeBuilder::collection("article")
    };
    builder = builder
        .with_title("Article")
        .with_field(text_field("title"));
    if !draft_and_publish {
        builder = builder.with_draft_and_publish(false);
    }
    let dt = builder.build();
    let type_id = dt.id.clone();
    let schema = SchemaRegistry::new(vec![dt]);
    let ctx: &'static SystemContext = Box::leak(Box::new(SystemContext::new(
        schema,
        fixture_system_config(),
    )));
    let leaked_dt = ctx.find_type(&type_id).unwrap();

    let repo = FakeDocumentInstanceRepository::new();
    let service = DocumentsServiceImpl::new(repo, ctx);
    let caller = CallerContext::system();

    (service, leaked_dt, caller)
}

#[tokio::test]
async fn ds_01_create_collection_draft_success() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Hello CMS".into(),
        ))),
    );

    let res = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await;

    assert!(res.is_ok(), "expected successful creation: {:?}", res);
    let inst = res.unwrap();
    assert_eq!(inst.audit.version, 1);
    assert_eq!(inst.audit.created_by, caller.actor);
    assert!(matches!(
        inst.content.publication_state,
        PublicationState::Draft
    ));
}

#[tokio::test]
async fn ds_02_create_blocked_for_second_singletype_instance() {
    let (service, doc_type, caller) = setup_service_fixture(true, true);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Initial".into(),
        ))),
    );

    let first = service
        .create(
            &caller,
            CreateDocumentCommand::new(&doc_type.id, fields.clone()),
        )
        .await;
    assert!(first.is_ok());

    let second = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await;

    assert!(matches!(
        second,
        Err(ApplicationError::Domain(
            DomainError::SingleTypeAlreadyExists(_)
        ))
    ));
}

#[tokio::test]
async fn ds_03_create_rejects_content_violating_field_constraints() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    // Title is required by text_field fixture; omit it to violate schema
    let fields = HashMap::new();

    let res = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await;

    assert!(matches!(res, Err(ApplicationError::Validation(_))));
}

#[tokio::test]
async fn ds_04_update_bumps_version_and_touches_audit_trail() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("V1".into()))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    let mut update_fields = HashMap::new();
    update_fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("V2".into()))),
    );

    let updated = service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, update_fields),
        )
        .await
        .unwrap();

    assert_eq!(updated.audit.version, 2);
    assert_eq!(updated.audit.updated_by, caller.actor);
}

#[tokio::test]
async fn ds_05_update_retains_unmentioned_fields() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Persistent Title".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    // Partial update with empty field changes
    let updated = service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, HashMap::new()),
        )
        .await
        .unwrap();

    assert!(updated.fields().contains_key(&test_attr_id("title")));
}

#[tokio::test]
async fn ds_06_publish_advances_revision_on_draftpublish_enabled_type() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Publishable".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    let published = service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();

    assert!(matches!(
        published.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));
}

#[tokio::test]
async fn ds_17_publish_rejects_already_published_document_without_pending_changes() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Clean Published".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();
    let published = service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();
    let version_before = published.audit.version;

    // DS-17: second publish without any pending draft changes is rejected
    let res = service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await;
    match res {
        Err(ApplicationError::Conflict { code, message, .. }) => {
            assert_eq!(code, ConflictCode::AlreadyPublished);
            assert!(
                message.contains("already published"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected Conflict, got {other:?}"),
    }

    let live = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap()
        .expect("live document must still exist");
    assert!(matches!(
        live.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));
    assert_eq!(live.audit.version, version_before);

    let snapshots = service
        .list_snapshots(&caller, ListSnapshotsCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap();
    assert!(
        snapshots.is_empty(),
        "no snapshot may be created: {snapshots:?}"
    );
}

#[tokio::test]
async fn ds_07_publish_rejects_document_of_draftpublish_disabled_type() {
    let (service, doc_type, caller) = setup_service_fixture(false, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Static".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    assert!(
        matches!(
            inst.content.publication_state,
            PublicationState::Published { revision: 1, .. }
        ),
        "expected created document to be immediately published with revision 1, got {:?}",
        inst.content.publication_state
    );

    let res = service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await;

    assert!(matches!(
        res,
        Err(ApplicationError::Conflict {
            code: ConflictCode::DraftAndPublishDisabled,
            ..
        })
    ));

    let mut update_fields = HashMap::new();
    update_fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Updated Static".into(),
        ))),
    );
    let updated = service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, update_fields),
        )
        .await
        .unwrap();

    assert!(
        matches!(
            updated.content.publication_state,
            PublicationState::Published { revision: 2, .. }
        ),
        "expected updated document to transition to published with revision 2, got {:?}",
        updated.content.publication_state
    );
}

#[tokio::test]
async fn ds_08_unpublish_transitions_to_draft_with_last_revision() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Lifecycle".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();
    service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();

    let draft = service
        .unpublish(
            &caller,
            UnpublishDocumentCommand::new(inst.id, &doc_type.id),
        )
        .await
        .unwrap();

    assert!(matches!(
        draft.content.publication_state,
        PublicationState::Unpublished {
            last_published_revision: 1,
            ..
        }
    ));

    // Verify Published view returns None after unpublish
    let published_view = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap();
    assert!(published_view.is_none());

    // Verify Working view returns the instance in Unpublished state
    let working_view = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst.id).with_view(VersionView::Working),
        )
        .await
        .unwrap()
        .expect("found in working view");
    assert!(matches!(
        working_view.content.publication_state,
        PublicationState::Unpublished {
            last_published_revision: 1,
            ..
        }
    ));
}

#[tokio::test]
async fn ds_09_unpublish_rejects_already_draft_instance() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Draft".into()))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    let res = service
        .unpublish(
            &caller,
            UnpublishDocumentCommand::new(inst.id, &doc_type.id),
        )
        .await;

    assert!(matches!(
        res,
        Err(ApplicationError::Domain(
            DomainError::InvalidStateTransition { .. }
        ))
    ));
}

#[tokio::test]
async fn ds_10_delete_removes_instance() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Disposable".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    service
        .delete(&caller, DeleteDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();

    let fetched = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap();
    assert!(fetched.is_none());

    let fetched_working = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst.id).with_view(VersionView::Working),
        )
        .await
        .unwrap();
    assert!(fetched_working.is_none());
}

#[tokio::test]
async fn ds_11_find_applies_filters_and_returns_total_count() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    for title in ["alpha", "beta", "alpha"] {
        let mut fields = HashMap::new();
        fields.insert(
            test_attr_id("title"),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(title.into()))),
        );
        service
            .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
            .await
            .unwrap();
    }

    let filter = FieldFilter {
        attribute_id: test_attr_id("title"),
        value: DomainValue::Primitive(PrimitiveValue::Text("alpha".into())),
    };

    let (items, total) = service
        .find(
            &caller,
            FindDocumentsCommand::new(&doc_type.id, Pagination::default())
                .with_filters(vec![filter]),
        )
        .await
        .unwrap();

    assert_eq!(total, 2);
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn ds_12_unauthorized_caller_denied_on_all_mutations() {
    let (service, doc_type, admin) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Secret".into(),
        ))),
    );
    let inst = service
        .create(&admin, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    let stranger = CallerContext::new(test_user_id("stranger"), vec![]);

    let update_res = service
        .update(
            &stranger,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, HashMap::new()),
        )
        .await;
    assert!(matches!(
        update_res,
        Err(ApplicationError::Unauthorized { .. })
    ));

    let delete_res = service
        .delete(&stranger, DeleteDocumentCommand::new(inst.id, &doc_type.id))
        .await;
    assert!(matches!(
        delete_res,
        Err(ApplicationError::Unauthorized { .. })
    ));

    let publish_res = service
        .publish(
            &stranger,
            PublishDocumentCommand::new(inst.id, &doc_type.id),
        )
        .await;
    assert!(matches!(
        publish_res,
        Err(ApplicationError::Unauthorized { .. })
    ));

    let unpublish_res = service
        .unpublish(
            &stranger,
            UnpublishDocumentCommand::new(inst.id, &doc_type.id),
        )
        .await;
    assert!(matches!(
        unpublish_res,
        Err(ApplicationError::Unauthorized { .. })
    ));
}

#[tokio::test]
async fn ds_13_discard_draft_reverts_to_published_state() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Original Published".into(),
        ))),
    );

    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    // 1. Publish to live
    let published = service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();
    assert!(matches!(
        published.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));

    // Attempting to discard draft when clean published fails
    let fail_discard = service
        .discard_draft(&caller, DiscardDraftCommand::new(inst.id, &doc_type.id))
        .await;
    assert!(matches!(
        fail_discard,
        Err(ApplicationError::Domain(
            DomainError::InvalidStateTransition { .. }
        ))
    ));

    // 2. Edit draft -> transitions to Modified state
    let mut update_fields = HashMap::new();
    update_fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Draft Modifications".into(),
        ))),
    );

    let modified = service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, update_fields),
        )
        .await
        .unwrap();
    assert!(matches!(
        modified.content.publication_state,
        PublicationState::Modified {
            live_revision: 1,
            ..
        }
    ));
    assert_eq!(
        modified.get_field(&test_attr_id("title")),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Draft Modifications".into())
        )))
    );

    // 3. Discard draft -> reverts to clean live Published copy
    let restored = service
        .discard_draft(&caller, DiscardDraftCommand::new(inst.id, &doc_type.id))
        .await
        .expect("discard draft succeeds");

    assert!(matches!(
        restored.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));
    assert_eq!(
        restored.get_field(&test_attr_id("title")),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Original Published".into())
        )))
    );

    // Verify subsequent find_by_id returns the clean published instance
    let fetched = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap()
        .expect("found");
    assert_eq!(
        fetched.get_field(&test_attr_id("title")),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Original Published".into())
        )))
    );
}

#[tokio::test]
async fn ds_14_restore_snapshot_loads_historical_revision_to_draft() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);

    // 1. Create Revision 1
    let mut fields_v1 = HashMap::new();
    fields_v1.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "V1 Title".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields_v1))
        .await
        .unwrap();

    let pub1 = service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();
    assert!(matches!(
        pub1.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));

    // 2. Update and Publish Revision 2
    let mut fields_v2 = HashMap::new();
    fields_v2.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "V2 Title".into(),
        ))),
    );
    service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, fields_v2),
        )
        .await
        .unwrap();

    let pub2 = service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();
    assert!(matches!(
        pub2.content.publication_state,
        PublicationState::Published { revision: 2, .. }
    ));

    // 3. Verify snapshot history via list_snapshots (newest first)
    // Under archive-on-replace, publishing revision 2 archives revision 1, leaving 1 snapshot.
    let snapshots = service
        .list_snapshots(&caller, ListSnapshotsCommand::new(&doc_type.id, inst.id))
        .await
        .expect("list snapshots ok");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].revision, 1);
    assert_eq!(snapshots[0].reason, SnapshotReason::Publish);

    // Edit working draft to create in-progress changes (Modified state)
    let mut fields_v3 = HashMap::new();
    fields_v3.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "V3 In-Progress Draft".into(),
        ))),
    );
    let modified = service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, fields_v3),
        )
        .await
        .unwrap();
    assert!(matches!(
        modified.content.publication_state,
        PublicationState::Modified {
            live_revision: 2,
            ..
        }
    ));

    // 4. Attempt to Restore Snapshot Revision 1 without confirming discard of pending edits (must fail with Conflict)
    let conflict_err = service
        .restore_snapshot(
            &caller,
            RestoreSnapshotCommand::new(inst.id, &doc_type.id, 1),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            conflict_err,
            ApplicationError::Conflict {
                code: ConflictCode::DraftHasPendingEdits,
                ..
            }
        ),
        "expected Conflict error when restoring snapshot over Modified edits without discard flag: {:?}",
        conflict_err
    );

    // 5. Restore Snapshot Revision 1 with explicit discard_pending_edits
    let restored = service
        .restore_snapshot(
            &caller,
            RestoreSnapshotCommand::new(inst.id, &doc_type.id, 1).with_discard_pending_edits(),
        )
        .await
        .expect("restore snapshot succeeds with discard_pending_edits");

    assert!(matches!(
        restored.content.publication_state,
        PublicationState::Modified {
            live_revision: 2,
            ..
        }
    ));
    assert_eq!(
        restored.get_field(&test_attr_id("title")),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("V1 Title".into())
        )))
    );

    // Verify snapshot list contains only the superseded Publish snapshot (no checkpoint created)
    let snapshots_after = service
        .list_snapshots(&caller, ListSnapshotsCommand::new(&doc_type.id, inst.id))
        .await
        .expect("list snapshots ok");
    assert_eq!(snapshots_after.len(), 1);
    assert_eq!(snapshots_after[0].reason, SnapshotReason::Publish);
    assert_eq!(snapshots_after[0].revision, 1);

    // Verify default view (Published) returns live revision 2
    let published_view = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap()
        .expect("found published view");
    assert!(matches!(
        published_view.content.publication_state,
        PublicationState::Published { revision: 2, .. }
    ));
    assert_eq!(
        published_view.get_field(&test_attr_id("title")),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("V2 Title".into())
        )))
    );

    // Verify Working view returns restored revision 1 in Modified state
    let working_view = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst.id).with_view(VersionView::Working),
        )
        .await
        .unwrap()
        .expect("found working view");
    assert!(matches!(
        working_view.content.publication_state,
        PublicationState::Modified {
            live_revision: 2,
            ..
        }
    ));
    assert_eq!(
        working_view.get_field(&test_attr_id("title")),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("V1 Title".into())
        )))
    );
}

#[tokio::test]
async fn ds_16_unpublish_draft_resolution_on_modified_document() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let title_attr = test_attr_id("title");

    // --- Case A: Unpublish with KeepDraft (default) ---
    let mut fields_a1 = HashMap::new();
    fields_a1.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Live V1 A".into(),
        ))),
    );
    let inst_a = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields_a1))
        .await
        .unwrap();

    service
        .publish(
            &caller,
            PublishDocumentCommand::new(inst_a.id, &doc_type.id),
        )
        .await
        .unwrap();

    let mut fields_a2 = HashMap::new();
    fields_a2.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Draft V2 A".into(),
        ))),
    );
    service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst_a.id, &doc_type.id, fields_a2),
        )
        .await
        .unwrap();

    let unpub_a = service
        .unpublish(
            &caller,
            UnpublishDocumentCommand::new(inst_a.id, &doc_type.id)
                .with_draft_resolution(UnpublishDraftResolution::KeepDraft),
        )
        .await
        .unwrap();

    assert!(matches!(
        unpub_a.content.publication_state,
        PublicationState::Unpublished {
            last_published_revision: 1,
            ..
        }
    ));
    // Kept draft edits
    assert_eq!(
        unpub_a.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Draft V2 A".into())
        )))
    );

    // Published view is None
    let pub_view_a = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst_a.id))
        .await
        .unwrap();
    assert!(pub_view_a.is_none());

    // Working view is Unpublished with draft edits
    let work_view_a = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst_a.id).with_view(VersionView::Working),
        )
        .await
        .unwrap()
        .expect("found working");
    assert_eq!(
        work_view_a.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Draft V2 A".into())
        )))
    );

    // Snapshots has the unpublish snapshot for Live V1
    let snaps_a = service
        .list_snapshots(&caller, ListSnapshotsCommand::new(&doc_type.id, inst_a.id))
        .await
        .unwrap();
    assert_eq!(snaps_a.len(), 1);
    assert_eq!(snaps_a[0].revision, 1);
    assert_eq!(snaps_a[0].reason, SnapshotReason::Unpublish);

    // --- Case B: Unpublish with ReplaceWithPublished ---
    let mut fields_b1 = HashMap::new();
    fields_b1.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Live V1 B".into(),
        ))),
    );
    let inst_b = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields_b1))
        .await
        .unwrap();

    service
        .publish(
            &caller,
            PublishDocumentCommand::new(inst_b.id, &doc_type.id),
        )
        .await
        .unwrap();

    let mut fields_b2 = HashMap::new();
    fields_b2.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Draft V2 B".into(),
        ))),
    );
    service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst_b.id, &doc_type.id, fields_b2),
        )
        .await
        .unwrap();

    let unpub_b = service
        .unpublish(
            &caller,
            UnpublishDocumentCommand::new(inst_b.id, &doc_type.id)
                .with_draft_resolution(UnpublishDraftResolution::ReplaceWithPublished),
        )
        .await
        .unwrap();

    assert!(matches!(
        unpub_b.content.publication_state,
        PublicationState::Unpublished {
            last_published_revision: 1,
            ..
        }
    ));
    // Replaced with published live content ("Live V1 B")
    assert_eq!(
        unpub_b.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Live V1 B".into())
        )))
    );

    // Working view is Unpublished with live content
    let work_view_b = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst_b.id).with_view(VersionView::Working),
        )
        .await
        .unwrap()
        .expect("found working");
    assert_eq!(
        work_view_b.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Live V1 B".into())
        )))
    );
}

#[tokio::test]
async fn ds_15_list_admin_headers_returns_lightweight_projections_and_enriches_relation() {
    let author_type = DocumentTypeBuilder::collection("author")
        .with_title("Authors")
        .with_draft_and_publish(false)
        .with_field(text_field("name"))
        .build();

    let author_type_id = author_type.id.clone();
    let author_attr = test_attr_id("author");
    let title_attr = test_attr_id("title");

    let article_type = DocumentTypeBuilder::collection("article")
        .with_title("Articles")
        .with_draft_and_publish(true)
        .with_field(text_field("title"))
        .with_relation(RelationDefinition {
            id: author_attr.clone(),
            relation_type: RelationType::HasOne,
            target: author_type_id.clone(),
        })
        .with_display_fields(vec![title_attr.clone(), author_attr.clone()])
        .build();

    let article_type_id = article_type.id.clone();
    let schema = SchemaRegistry::new(vec![article_type, author_type]);
    let ctx: &'static SystemContext = Box::leak(Box::new(SystemContext::new(
        schema,
        fixture_system_config(),
    )));
    let leaked_article = ctx.find_type(&article_type_id).unwrap();
    let leaked_author = ctx.find_type(&author_type_id).unwrap();

    let repo = FakeDocumentInstanceRepository::new();
    let service = DocumentsServiceImpl::new(repo, ctx);
    let caller = CallerContext::system();

    // 1. Create Author "Ada Lovelace"
    let mut author_fields = HashMap::new();
    author_fields.insert(
        test_attr_id("name"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Ada Lovelace".into(),
        ))),
    );
    let author = service
        .create(
            &caller,
            CreateDocumentCommand::new(&leaked_author.id, author_fields),
        )
        .await
        .unwrap();

    // 2. Create Article 1 connected to Author
    let mut art1_fields = HashMap::new();
    art1_fields.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Note G".into(),
        ))),
    );
    let mut art1_rels = HashMap::new();
    art1_rels.insert(
        author_attr.clone(),
        RelationAction::Connect(vec![author.id]),
    );

    let art1 = service
        .create(
            &caller,
            CreateDocumentCommand::new(&leaked_article.id, art1_fields).with_relations(art1_rels),
        )
        .await
        .unwrap();

    // Publish Article 1 and then update it to become Modified
    service
        .publish(
            &caller,
            PublishDocumentCommand::new(art1.id, &leaked_article.id),
        )
        .await
        .unwrap();

    let mut update_fields = HashMap::new();
    update_fields.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Note G Revised".into(),
        ))),
    );
    service
        .update(
            &caller,
            UpdateDocumentCommand::new(art1.id, &leaked_article.id, update_fields),
        )
        .await
        .unwrap();

    // 3. Create Article 2 (Draft, no author)
    let mut art2_fields = HashMap::new();
    art2_fields.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Sketch".into(),
        ))),
    );
    service
        .create(
            &caller,
            CreateDocumentCommand::new(&leaked_article.id, art2_fields),
        )
        .await
        .unwrap();

    // 4. Query list_admin_headers without filter (all articles)
    let (headers, total) = service
        .list_admin_headers(
            &caller,
            ListAdminHeadersCommand::new(&leaked_article.id, Pagination::default()),
        )
        .await
        .expect("list admin headers ok");

    assert_eq!(total, 2);
    assert_eq!(headers.len(), 2);

    let art1_header = headers.iter().find(|h| h.id == art1.id).unwrap();
    assert_eq!(art1_header.status, DocumentStatus::Modified);
    // Phase 2 enrichment: author relation was resolved to "Ada Lovelace"
    assert_eq!(
        art1_header.display_values.get(&title_attr),
        Some(&"Note G Revised".to_string())
    );
    assert_eq!(
        art1_header.display_values.get(&author_attr),
        Some(&"Ada Lovelace".to_string())
    );
    assert_eq!(art1_header.display_title, "Note G Revised — Ada Lovelace");

    let art2_header = headers.iter().find(|h| h.id != art1.id).unwrap();
    assert_eq!(art2_header.status, DocumentStatus::Draft);
    assert_eq!(
        art2_header.display_values.get(&title_attr),
        Some(&"Sketch".to_string())
    );
    assert_eq!(art2_header.display_title, "Sketch");

    // 5. Query with status_filter = Draft
    let (draft_headers, draft_total) = service
        .list_admin_headers(
            &caller,
            ListAdminHeadersCommand::new(&leaked_article.id, Pagination::default())
                .with_status_filter(DocumentStatus::Draft),
        )
        .await
        .expect("list admin headers with filter ok");

    assert_eq!(draft_total, 1);
    assert_eq!(draft_headers.len(), 1);
    assert_eq!(draft_headers[0].status, DocumentStatus::Draft);
}

#[tokio::test]
async fn test_version_views_across_all_four_document_states() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let title_attr = test_attr_id("title");

    // 1. State: Draft
    let mut draft_fields = HashMap::new();
    draft_fields.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Draft Title".into(),
        ))),
    );
    let inst = service
        .create(
            &caller,
            CreateDocumentCommand::new(&doc_type.id, draft_fields),
        )
        .await
        .unwrap();

    let pub_view_1 = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap();
    assert!(
        pub_view_1.is_none(),
        "Draft must not be visible in Published tab"
    );

    let work_view_1 = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst.id).with_view(VersionView::Working),
        )
        .await
        .unwrap()
        .expect("Draft must be visible in Working tab");
    assert!(matches!(
        work_view_1.content.publication_state,
        PublicationState::Draft
    ));
    assert_eq!(
        work_view_1.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Draft Title".into())
        )))
    );

    // 2. State: Published
    service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();

    let pub_view_2 = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap()
        .expect("Published instance must be visible in Published tab");
    assert!(matches!(
        pub_view_2.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));
    assert_eq!(
        pub_view_2.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Draft Title".into())
        )))
    );

    let work_view_2 = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst.id).with_view(VersionView::Working),
        )
        .await
        .unwrap()
        .expect("Published instance must be visible in Working tab");
    assert!(matches!(
        work_view_2.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));
    assert_eq!(
        work_view_2.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Draft Title".into())
        )))
    );

    // 3. State: Modified (Live Published + Pending Draft Edits)
    let mut modified_fields = HashMap::new();
    modified_fields.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Modified Title".into(),
        ))),
    );
    service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, modified_fields),
        )
        .await
        .unwrap();

    let pub_view_3 = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap()
        .expect("Live published content must remain visible in Published tab");
    assert!(matches!(
        pub_view_3.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));
    assert_eq!(
        pub_view_3.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Draft Title".into())
        )))
    );

    let work_view_3 = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst.id).with_view(VersionView::Working),
        )
        .await
        .unwrap()
        .expect("Pending draft content must be visible in Working tab");
    assert!(matches!(
        work_view_3.content.publication_state,
        PublicationState::Modified {
            live_revision: 1,
            ..
        }
    ));
    assert_eq!(
        work_view_3.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Modified Title".into())
        )))
    );

    // 4. State: Unpublished
    service
        .unpublish(
            &caller,
            UnpublishDocumentCommand::new(inst.id, &doc_type.id),
        )
        .await
        .unwrap();

    let pub_view_4 = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap();
    assert!(
        pub_view_4.is_none(),
        "Unpublished instance must not be visible in Published tab"
    );

    let work_view_4 = service
        .find_by_id(
            &caller,
            FindByIdCommand::new(&doc_type.id, inst.id).with_view(VersionView::Working),
        )
        .await
        .unwrap()
        .expect("Unpublished instance must be visible in Working tab");
    assert!(matches!(
        work_view_4.content.publication_state,
        PublicationState::Unpublished {
            last_published_revision: 1,
            ..
        }
    ));
    assert_eq!(
        work_view_4.get_field(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Modified Title".into())
        )))
    );
}

#[tokio::test]
async fn ds_18_get_snapshot_returns_historical_revision() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let title_attr = test_attr_id("title");

    // 1. Create and publish revision 1
    let mut f1 = HashMap::new();
    f1.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Revision 1".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, f1))
        .await
        .unwrap();
    service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();

    // 2. Update to draft and publish revision 2 (archives revision 1)
    let mut f2 = HashMap::new();
    f2.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Revision 2".into(),
        ))),
    );
    service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, f2),
        )
        .await
        .unwrap();
    service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await
        .unwrap();

    // 3. Inspect historical revision 1 snapshot via get_snapshot
    let snap = service
        .get_snapshot(&caller, GetSnapshotCommand::new(&doc_type.id, inst.id, 1))
        .await
        .unwrap();
    assert_eq!(snap.revision, 1);
    assert_eq!(snap.document_id, inst.id);
    assert_eq!(
        snap.fields.get(&title_attr),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("Revision 1".into())
        )))
    );

    // 4. Non-existent revision returns NotFound
    let res = service
        .get_snapshot(&caller, GetSnapshotCommand::new(&doc_type.id, inst.id, 99))
        .await;
    assert!(matches!(
        res,
        Err(ApplicationError::NotFound {
            entity: "DocumentSnapshot",
            ..
        })
    ));
}

#[tokio::test]
async fn test_update_occ_version_mismatch_conflict() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let title_attr = test_attr_id("title");

    let mut fields = HashMap::new();
    fields.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Original".into(),
        ))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();
    assert_eq!(inst.audit.version, 1);

    // Update with correct expected_version succeeds
    let mut u1 = HashMap::new();
    u1.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
            "Updated".into(),
        ))),
    );
    let updated = service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, u1).with_expected_version(1),
        )
        .await
        .unwrap();
    assert_eq!(updated.audit.version, 2);

    // Update with stale expected_version fails with ConflictCode::VersionMismatch
    let mut u2 = HashMap::new();
    u2.insert(
        title_attr.clone(),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Stale".into()))),
    );
    let res = service
        .update(
            &caller,
            UpdateDocumentCommand::new(inst.id, &doc_type.id, u2).with_expected_version(1),
        )
        .await;
    assert!(matches!(
        res,
        Err(ApplicationError::Conflict {
            code: ConflictCode::VersionMismatch,
            ..
        })
    ));
}

#[tokio::test]
async fn test_restore_snapshot_on_draft_fails_immediately_with_invalid_state_transition() {
    let (service, doc_type, caller) = setup_service_fixture(true, false);
    let title_attr = test_attr_id("title");

    let mut fields = HashMap::new();
    fields.insert(
        title_attr,
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Draft".into()))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    // Restoring snapshot on a fresh draft fails with InvalidStateTransition (not NotFound)
    let res = service
        .restore_snapshot(
            &caller,
            RestoreSnapshotCommand::new(inst.id, &doc_type.id, 1),
        )
        .await;
    assert!(matches!(
        res,
        Err(ApplicationError::Domain(
            DomainError::InvalidStateTransition { .. }
        ))
    ));
}
