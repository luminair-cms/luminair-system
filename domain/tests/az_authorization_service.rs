//! Spec: docs/specs/domain/authorization-service.md — generated from behavior specification rules.

use domain::auth::{AuthorizationService, Permission, Role, RoleId};
use domain::content::DocumentInstance;
use domain::test_support::{admin_role, fixture_document_instance, test_doc_type_id, test_user_id};
use uuid::Uuid;

enum TargetInstance<'a> {
    Owner(&'a DocumentInstance),
    NonOwner(&'a DocumentInstance),
    None,
}

struct Case<'a> {
    id: &'static str,
    target: TargetInstance<'a>,
    action: Permission,
    roles: Vec<Role>,
    expected: bool,
}

#[test]
fn test_az_authorization_decision_table() {
    let owner_user = test_user_id("owner");
    let other_user = test_user_id("other");
    let article_type = test_doc_type_id("article");
    let news_type = test_doc_type_id("news");
    let instance = fixture_document_instance("article", Some("owner"));

    let reader_article_role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "reader_article".into(),
        description: None,
        permissions: vec![Permission::ReadDocument(Some(article_type.clone()))],
    };

    let reader_news_role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "reader_news".into(),
        description: None,
        permissions: vec![Permission::ReadDocument(Some(news_type))],
    };

    let wildcard_reader_role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "global_reader".into(),
        description: None,
        permissions: vec![Permission::ReadDocument(None)],
    };

    let publisher_article_role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "publisher_article".into(),
        description: None,
        permissions: vec![Permission::PublishDocument(Some(article_type.clone()))],
    };

    let wildcard_publisher_role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "global_publisher".into(),
        description: None,
        permissions: vec![Permission::PublishDocument(None)],
    };

    let creator_article_role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "creator_article".into(),
        description: None,
        permissions: vec![Permission::CreateDocument(Some(article_type.clone()))],
    };

    let schema_admin_role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "schema_admin".into(),
        description: None,
        permissions: vec![Permission::ManageSchema],
    };

    let user_admin_role = Role {
        id: RoleId::new(Uuid::now_v7()),
        name: "user_admin".into(),
        description: None,
        permissions: vec![Permission::ManageUsers],
    };

    let cases = vec![
        Case { id: "AZ-01", target: TargetInstance::Owner(&instance), action: Permission::ReadDocument(Some(article_type.clone())), roles: vec![], expected: true },
        Case { id: "AZ-02", target: TargetInstance::Owner(&instance), action: Permission::UpdateDocument(Some(article_type.clone())), roles: vec![], expected: true },
        Case { id: "AZ-03", target: TargetInstance::Owner(&instance), action: Permission::DeleteDocument(Some(article_type.clone())), roles: vec![], expected: true },
        Case { id: "AZ-04", target: TargetInstance::Owner(&instance), action: Permission::PublishDocument(Some(article_type.clone())), roles: vec![], expected: false },
        Case { id: "AZ-05", target: TargetInstance::NonOwner(&instance), action: Permission::ReadDocument(Some(article_type.clone())), roles: vec![], expected: false },
        Case { id: "AZ-06", target: TargetInstance::NonOwner(&instance), action: Permission::UpdateDocument(Some(article_type.clone())), roles: vec![], expected: false },
        Case { id: "AZ-07", target: TargetInstance::NonOwner(&instance), action: Permission::DeleteDocument(Some(article_type.clone())), roles: vec![], expected: false },
        Case { id: "AZ-08", target: TargetInstance::NonOwner(&instance), action: Permission::PublishDocument(Some(article_type.clone())), roles: vec![], expected: false },
        Case { id: "AZ-09", target: TargetInstance::NonOwner(&instance), action: Permission::ReadDocument(Some(article_type.clone())), roles: vec![reader_article_role], expected: true },
        Case { id: "AZ-10", target: TargetInstance::NonOwner(&instance), action: Permission::ReadDocument(Some(article_type.clone())), roles: vec![reader_news_role], expected: false },
        Case { id: "AZ-11", target: TargetInstance::NonOwner(&instance), action: Permission::ReadDocument(Some(article_type.clone())), roles: vec![wildcard_reader_role], expected: true },
        Case { id: "AZ-12", target: TargetInstance::Owner(&instance), action: Permission::PublishDocument(Some(article_type.clone())), roles: vec![publisher_article_role], expected: true },
        Case { id: "AZ-13", target: TargetInstance::NonOwner(&instance), action: Permission::PublishDocument(Some(article_type.clone())), roles: vec![wildcard_publisher_role], expected: true },
        Case { id: "AZ-14", target: TargetInstance::None, action: Permission::CreateDocument(Some(article_type.clone())), roles: vec![creator_article_role], expected: true },
        Case { id: "AZ-15", target: TargetInstance::None, action: Permission::CreateDocument(Some(article_type.clone())), roles: vec![], expected: false },
        Case { id: "AZ-16", target: TargetInstance::None, action: Permission::ManageSchema, roles: vec![schema_admin_role], expected: true },
        Case { id: "AZ-17", target: TargetInstance::None, action: Permission::ManageRoles, roles: vec![user_admin_role], expected: false },
        Case { id: "AZ-18", target: TargetInstance::NonOwner(&instance), action: Permission::DeleteDocument(Some(article_type)), roles: vec![admin_role()], expected: true },
    ];

    let mut failures = Vec::new();

    for case in cases {
        let (user, inst_opt) = match case.target {
            TargetInstance::Owner(inst) => (&owner_user, Some(inst)),
            TargetInstance::NonOwner(inst) => (&other_user, Some(inst)),
            TargetInstance::None => (&other_user, None),
        };

        let result = AuthorizationService::can(user, &case.action, inst_opt, &case.roles);

        if result != case.expected {
            failures.push(format!("{}: expected {}, got {}", case.id, case.expected, result));
        }
    }

    assert!(failures.is_empty(), "Failed cases:\n- {}", failures.join("\n- "));
}
