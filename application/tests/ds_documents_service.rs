use std::collections::HashMap;

use application::commands::documents::*;
use application::context::CallerContext;
use application::errors::ApplicationError;
use application::services::{DocumentsService, DocumentsServiceImpl};
use application::test_support::FakeDocumentInstanceRepository;
use domain::content::{
    ContentValue, DomainValue, FieldFilter, Pagination, PrimitiveValue,
    PublicationState,
};
use domain::errors::DomainError;
use domain::schema::{DocumentType, SchemaRegistry};
use domain::system::SystemContext;
use domain::test_support::{
    fixture_system_config, test_attr_id, test_user_id, text_field,
    DocumentTypeBuilder,
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
    assert_eq!(inst.audit.created_by, Some(caller.user_id));
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
        .create(&caller, CreateDocumentCommand::new(&doc_type.id, fields.clone()))
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
    assert_eq!(updated.audit.updated_by, Some(caller.user_id));
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

    assert!(updated.content.fields.contains_key(&test_attr_id("title")));
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
        PublicationState::Draft {
            last_published_revision: Some(1)
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
