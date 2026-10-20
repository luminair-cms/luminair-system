use std::collections::HashMap;

use application::commands::documents::*;
use application::context::CallerContext;
use application::errors::ApplicationError;
use application::services::{DocumentsService, DocumentsServiceImpl};
use application::test_support::FakeDocumentInstanceRepository;
use domain::content::{
    ContentValue, DocumentStatus, DomainValue, FieldFilter, Pagination, PrimitiveValue,
    PublicationState,
};
use domain::errors::DomainError;
use domain::schema::{DocumentType, RelationDefinition, RelationType, SchemaRegistry};
use domain::system::SystemContext;
use domain::test_support::{
    fixture_system_config, test_attr_id, test_user_id, text_field, DocumentTypeBuilder,
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
    builder = builder.with_title("Article").with_field(text_field("title"));
    if !draft_and_publish {
        builder = builder.with_draft_and_publish(false);
    }
    let dt = builder.build();
    let type_id = dt.id.clone();
    let schema = SchemaRegistry::new(vec![dt]);
    let ctx: &'static SystemContext =
        Box::leak(Box::new(SystemContext::new(schema, fixture_system_config())));
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
        PublicationState::Draft {
            last_published_revision: None
        }
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
        Err(ApplicationError::Domain(DomainError::SingleTypeAlreadyExists(_)))
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
async fn ds_07_publish_rejects_document_of_draftpublish_disabled_type() {
    let (service, doc_type, caller) = setup_service_fixture(false, false);
    let mut fields = HashMap::new();
    fields.insert(
        test_attr_id("title"),
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Static".into()))),
    );
    let inst = service
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields))
        .await
        .unwrap();

    let res = service
        .publish(&caller, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await;

    assert!(matches!(res, Err(ApplicationError::Conflict(_))));
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
        Err(ApplicationError::Domain(DomainError::InvalidStateTransition { .. }))
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
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("Secret".into()))),
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
    assert!(matches!(update_res, Err(ApplicationError::Unauthorized { .. })));

    let delete_res = service
        .delete(&stranger, DeleteDocumentCommand::new(inst.id, &doc_type.id))
        .await;
    assert!(matches!(delete_res, Err(ApplicationError::Unauthorized { .. })));

    let publish_res = service
        .publish(&stranger, PublishDocumentCommand::new(inst.id, &doc_type.id))
        .await;
    assert!(matches!(publish_res, Err(ApplicationError::Unauthorized { .. })));

    let unpublish_res = service
        .unpublish(
            &stranger,
            UnpublishDocumentCommand::new(inst.id, &doc_type.id),
        )
        .await;
    assert!(matches!(unpublish_res, Err(ApplicationError::Unauthorized { .. })));
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
        Err(ApplicationError::Domain(DomainError::InvalidStateTransition { .. }))
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
        PublicationState::Modified { live_revision: 1, .. }
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
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("V1 Title".into()))),
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
        ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text("V2 Title".into()))),
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
    let snapshots = service
        .list_snapshots(&caller, ListSnapshotsCommand::new(&doc_type.id, inst.id))
        .await
        .expect("list snapshots ok");
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].revision, 2);
    assert_eq!(snapshots[1].revision, 1);

    // 4. Restore Snapshot Revision 1
    let restored = service
        .restore_snapshot(
            &caller,
            RestoreSnapshotCommand::new(inst.id, &doc_type.id, 1),
        )
        .await
        .expect("restore snapshot succeeds");

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

    // Verify fetched working copy has restored content
    let fetched = service
        .find_by_id(&caller, FindByIdCommand::new(&doc_type.id, inst.id))
        .await
        .unwrap()
        .expect("found");
    assert_eq!(
        fetched.get_field(&test_attr_id("title")),
        Some(&ContentValue::Scalar(DomainValue::Primitive(
            PrimitiveValue::Text("V1 Title".into())
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
    let ctx: &'static SystemContext =
        Box::leak(Box::new(SystemContext::new(schema, fixture_system_config())));
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
