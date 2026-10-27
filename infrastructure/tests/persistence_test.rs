//! Integration tests for schema naming, query compilation, codecs, and database repositories.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use indexmap::IndexMap;
use rust_decimal::Decimal;
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

use domain::auth::access_request::AccessRequestStatus;
use domain::auth::policy::AccessRequestPolicy;
use domain::auth::ports::{AccessRequestRepository, RoleRepository, UserRoleAssignmentRepository};
use domain::auth::role::{Permission, Role, RoleId, UserRoleAssignment, UserRoleAssignmentId};
use domain::auth::user::{ActorId, UserId};
use domain::common::{DisplayName, Email};
use domain::content::header::{
    DocumentSnapshot, DocumentStatus, SnapshotId, SnapshotReason, VersionView,
};
use domain::content::instance::{
    DocumentInstance, DocumentInstanceId, PublicationState, ResolvedRelation,
};
use domain::content::ports::{DocumentInstanceRepository, DocumentSnapshotRepository, Pagination};
use domain::content::values::{ContentValue, DomainValue, PrimitiveValue};
use domain::schema::{AttributeId, DocumentTypeId};
use domain::system::{LocaleId, SystemContext};

use infrastructure::persistence::migration::{
    ROLE_ADMIN_ID, SafetyPolicy, run_static_migrations, sync_dynamic_schemas,
};
use infrastructure::persistence::naming::SchemaNaming;
use infrastructure::persistence::query::common::{compile_insert, compile_select};
use infrastructure::persistence::query::headers::{
    find_display_titles_by_ids_query, list_draft_enabled_headers_query,
};
use infrastructure::persistence::query::junction::insert_links_batch_query;
use infrastructure::persistence::query::wide_table::{find_published_query, select_by_id_query};
use infrastructure::persistence::repositories::codec::{
    deserialize_fields_json, deserialize_relations_json, serialize_fields_json,
    serialize_relations_json,
};
use infrastructure::persistence::repositories::{
    SqlxAccessRequestRepository, SqlxDocumentInstanceRepository, SqlxDocumentSnapshotRepository,
    SqlxRoleRepository, SqlxUserRoleAssignmentRepository,
};
use infrastructure::schema_loader::load_system_context_from_dir;

// ---------------------------------------------------------------------------
// Helper Functions - Value Constructors & Fixtures
// ---------------------------------------------------------------------------

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("cargo manifest dir must have a parent workspace directory")
        .to_path_buf()
}

fn load_test_system_context() -> &'static SystemContext {
    let schemas_dir = workspace_root().join("config/schemas");
    let ctx = load_system_context_from_dir(&schemas_dir)
        .expect("system context must load successfully from config/schemas");
    Box::leak(Box::new(ctx))
}

fn text_val(s: &str) -> ContentValue {
    ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(s.to_string())))
}

fn uid_val(s: &str) -> ContentValue {
    ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Uid(s.to_string())))
}

fn int_val(i: i64) -> ContentValue {
    ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Integer(i)))
}

fn dec_val(d: Decimal) -> ContentValue {
    ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Decimal(d)))
}

fn create_test_fields_map() -> HashMap<AttributeId, ContentValue> {
    let mut fields = HashMap::new();
    fields.insert(
        AttributeId::try_new("title").expect("valid attr id"),
        text_val("Testing Rust Persistence"),
    );
    fields.insert(
        AttributeId::try_new("slug").expect("valid attr id"),
        uid_val("testing-rust-persistence"),
    );

    let mut summary_map = HashMap::new();
    summary_map.insert(
        LocaleId::try_new("en").expect("valid locale"),
        "Clean architecture in Rust".to_string(),
    );
    fields.insert(
        AttributeId::try_new("summary").expect("valid attr id"),
        ContentValue::LocalizedText(summary_map),
    );
    fields.insert(
        AttributeId::try_new("body").expect("valid attr id"),
        text_val("Full article content here..."),
    );
    fields.insert(
        AttributeId::try_new("views").expect("valid attr id"),
        int_val(100),
    );
    fields.insert(
        AttributeId::try_new("priority").expect("valid attr id"),
        int_val(5),
    );
    fields.insert(
        AttributeId::try_new("price").expect("valid attr id"),
        dec_val(Decimal::new(1999, 2)),
    );

    let mut meta = HashMap::new();
    meta.insert("author".to_string(), PrimitiveValue::Text("Luminair".into()));
    meta.insert("views".to_string(), PrimitiveValue::Integer(42));
    fields.insert(
        AttributeId::try_new("metadata").expect("valid attr id"),
        ContentValue::Scalar(DomainValue::Json(meta)),
    );
    fields
}

fn create_test_relations_map(
) -> (AttributeId, DocumentInstanceId, HashMap<AttributeId, Vec<ResolvedRelation>>) {
    let cat_attr = AttributeId::try_new("category").expect("valid attr id");
    let cat_id = DocumentInstanceId::new(Uuid::now_v7());
    let mut relations = HashMap::new();
    relations.insert(
        cat_attr.clone(),
        vec![ResolvedRelation {
            attribute_id: cat_attr.clone(),
            target_instance_id: cat_id,
        }],
    );
    (cat_attr, cat_id, relations)
}

fn create_article_display_values(title: &str, category: &str) -> IndexMap<AttributeId, String> {
    let mut display_values = IndexMap::new();
    display_values.insert(
        AttributeId::try_new("title").expect("valid attr id"),
        title.to_string(),
    );
    display_values.insert(
        AttributeId::try_new("category").expect("valid attr id"),
        category.to_string(),
    );
    display_values
}

fn create_category_display_values(name: &str) -> IndexMap<AttributeId, String> {
    let mut display_values = IndexMap::new();
    display_values.insert(
        AttributeId::try_new("name").expect("valid attr id"),
        name.to_string(),
    );
    display_values
}

// ---------------------------------------------------------------------------
// Helper Functions - Database Provisioning
// ---------------------------------------------------------------------------

async fn setup_isolated_db() -> Option<(PgPool, &'static SystemContext, &'static SchemaNaming)> {
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => return None,
    };

    let schema_name = format!("test_persist_{}", Uuid::now_v7().simple());
    let init_pool = sqlx::PgPool::connect(&db_url).await.ok()?;
    let create_schema_sql = format!("CREATE SCHEMA \"{schema_name}\"");
    sqlx::query(AssertSqlSafe(create_schema_sql.as_str()))
        .execute(&init_pool)
        .await
        .ok()?;

    let schema_clone = schema_name.clone();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _meta| {
            let s = schema_clone.clone();
            Box::pin(async move {
                let set_path_sql = format!("SET search_path TO \"{s}\"");
                sqlx::query(AssertSqlSafe(set_path_sql.as_str()))
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&db_url)
        .await
        .ok()?;

    let ctx = load_test_system_context();
    let naming = SchemaNaming::from_registry(&ctx.schema);

    run_static_migrations(&pool).await.ok()?;
    sync_dynamic_schemas(&pool, &ctx.schema, SafetyPolicy::AdditiveOnly)
        .await
        .ok()?;

    Some((pool, ctx, naming))
}

// ---------------------------------------------------------------------------
// Helper Functions - Live Repository Step Assertions
// ---------------------------------------------------------------------------

async fn assert_roles_behavior(role_repo: &SqlxRoleRepository) -> (Role, Role) {
    let roles = role_repo.find_all().await.expect("find_all roles");
    assert!(roles.len() >= 4, "expected at least 4 seeded built-in roles");

    let admin_role = role_repo
        .find_by_name("admin")
        .await
        .expect("query admin role")
        .expect("admin role must exist");
    assert_eq!(*admin_role.id, ROLE_ADMIN_ID);

    let custom_role_id = RoleId::new(Uuid::now_v7());
    let custom_role = Role {
        id: custom_role_id,
        name: "moderator".to_string(),
        description: Some("Community moderator".to_string()),
        permissions: vec![Permission::ManageUsers, Permission::ReadDocument(None)],
    };
    role_repo.save(&custom_role).await.expect("save custom role");

    let found_custom = role_repo
        .find_by_id(custom_role_id)
        .await
        .expect("find custom role")
        .expect("custom role found");
    assert_eq!(found_custom.name, "moderator");
    assert_eq!(found_custom.permissions.len(), 2);

    let multi_roles = role_repo
        .find_by_ids(&[admin_role.id, custom_role_id])
        .await
        .expect("find_by_ids");
    assert_eq!(multi_roles.len(), 2);

    (admin_role, custom_role)
}

async fn assert_user_role_assignment_behavior(
    assignment_repo: &SqlxUserRoleAssignmentRepository,
    admin_role_id: RoleId,
    custom_role_id: RoleId,
    admin_actor: &ActorId,
) {
    let test_user_id = UserId::try_new("usr_test_user_01").expect("valid user id");

    let assignment1 = UserRoleAssignment {
        id: UserRoleAssignmentId::new(Uuid::now_v7()),
        user_id: test_user_id.clone(),
        role_id: admin_role_id,
        granted_at: Utc::now(),
        granted_by: Some(admin_actor.clone()),
    };
    assignment_repo
        .save(&assignment1)
        .await
        .expect("save assignment");

    let has_admin = assignment_repo
        .exists_admin(admin_role_id)
        .await
        .expect("exists_admin");
    assert!(has_admin);

    let user_assignments = assignment_repo
        .find_by_user(&test_user_id)
        .await
        .expect("find_by_user");
    assert_eq!(user_assignments.len(), 1);

    let assignment2 = UserRoleAssignment {
        id: UserRoleAssignmentId::new(Uuid::now_v7()),
        user_id: test_user_id.clone(),
        role_id: custom_role_id,
        granted_at: Utc::now(),
        granted_by: Some(admin_actor.clone()),
    };
    assignment_repo
        .save_all(std::slice::from_ref(&assignment2))
        .await
        .expect("save_all");

    let updated_assignments = assignment_repo
        .find_by_user(&test_user_id)
        .await
        .expect("find_by_user after save_all");
    assert_eq!(updated_assignments.len(), 2);

    assignment_repo
        .delete(assignment1.id)
        .await
        .expect("delete assignment");
    let after_delete = assignment_repo
        .find_by_user(&test_user_id)
        .await
        .expect("find_by_user after delete");
    assert_eq!(after_delete.len(), 1);
    assert_eq!(after_delete[0].id, assignment2.id);
}

async fn assert_access_request_behavior(
    access_repo: &SqlxAccessRequestRepository,
    custom_role_id: RoleId,
) {
    let applicant_id = UserId::try_new("usr_applicant_01").expect("valid applicant id");
    let applicant_email = Email::try_new("applicant@example.com").ok();
    let applicant_name = DisplayName::try_new("Applicant User").ok();

    let mut request = AccessRequestPolicy::create_request(
        applicant_id.clone(),
        applicant_email,
        applicant_name,
        &[],
        None,
        Utc::now(),
    )
    .expect("create_request succeeds");

    access_repo
        .save(&request)
        .await
        .expect("save pending request");

    let found_req = access_repo
        .find_by_id(request.id)
        .await
        .expect("find_by_id")
        .expect("request found");
    assert_eq!(found_req.status, AccessRequestStatus::Pending);

    let active = access_repo
        .find_active_by_user(&applicant_id)
        .await
        .expect("find_active")
        .expect("active request found");
    assert_eq!(active.id, request.id);

    let pending_list = access_repo.find_pending().await.expect("find_pending");
    assert!(pending_list.iter().any(|r| r.id == request.id));

    let reviewer = ActorId::User(UserId::try_new("usr_approver_01").expect("valid reviewer"));
    let new_assignments = request
        .approve(reviewer, vec![custom_role_id], Utc::now())
        .expect("approve request");
    assert_eq!(new_assignments.len(), 1);

    access_repo
        .save(&request)
        .await
        .expect("save approved request");

    let by_user = access_repo
        .find_by_user(&applicant_id)
        .await
        .expect("find_by_user")
        .expect("request found");
    assert_eq!(by_user.status, AccessRequestStatus::Approved);
    assert_eq!(by_user.assigned_roles, vec![custom_role_id]);
}

async fn assert_lifecycle_draft_creation(
    instance_repo: &SqlxDocumentInstanceRepository,
    category: &DocumentInstance,
    article: &DocumentInstance,
    category_type_id: &DocumentTypeId,
    article_type_id: &DocumentTypeId,
) {
    let cat_display = create_category_display_values("Engineering");
    instance_repo
        .save_draft(category, &cat_display, "Engineering")
        .await
        .expect("save_draft category");

    let art_display =
        create_article_display_values("Hexagonal Architecture in Rust", "Engineering");
    instance_repo
        .save_draft(article, &art_display, "Hexagonal Architecture in Rust")
        .await
        .expect("save_draft article");

    let working_article = instance_repo
        .find_by_id(article_type_id, article.id, VersionView::Working)
        .await
        .expect("find_by_id working")
        .expect("article exists in working view");
    assert_eq!(
        working_article.content.publication_state,
        PublicationState::Draft
    );

    let published_article = instance_repo
        .find_by_id(article_type_id, article.id, VersionView::Published)
        .await
        .expect("find_by_id published");
    assert!(
        published_article.is_none(),
        "draft article must not be in published view"
    );

    let working_category = instance_repo
        .find_by_id(category_type_id, category.id, VersionView::Working)
        .await
        .expect("find_by_id working category")
        .expect("category exists in working view");
    assert_eq!(
        working_category.content.publication_state,
        PublicationState::Draft
    );
}

async fn assert_lifecycle_publish_and_relations(
    instance_repo: &SqlxDocumentInstanceRepository,
    category: &mut DocumentInstance,
    article: &mut DocumentInstance,
    category_type_id: &DocumentTypeId,
    article_type_id: &DocumentTypeId,
    author_actor: &ActorId,
) {
    let cat_display = create_category_display_values("Engineering");
    category
        .publish(author_actor.clone(), Utc::now())
        .expect("category.publish");
    instance_repo
        .publish(category, 1, &cat_display, "Engineering", None)
        .await
        .expect("publish category");

    let art_display =
        create_article_display_values("Hexagonal Architecture in Rust", "Engineering");
    let rev1 = article
        .publish(author_actor.clone(), Utc::now())
        .expect("article.publish");
    assert_eq!(rev1, 1);
    instance_repo
        .publish(
            article,
            1,
            &art_display,
            "Hexagonal Architecture in Rust",
            None,
        )
        .await
        .expect("publish article");

    let pub_article = instance_repo
        .find_by_id(article_type_id, article.id, VersionView::Published)
        .await
        .expect("find_by_id published")
        .expect("published article found");
    assert!(matches!(
        pub_article.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));

    let pub_page = instance_repo
        .find_published(article_type_id, Pagination::default(), vec![])
        .await
        .expect("find_published");
    assert!(pub_page.items.iter().any(|item| item.id == article.id));

    let cat_attr = AttributeId::try_new("category").expect("valid attr id");
    let art_relations = instance_repo
        .fetch_relations(article_type_id, std::slice::from_ref(&cat_attr), &[article.id])
        .await
        .expect("fetch_relations article.category");
    let resolved_cat = art_relations
        .get(&cat_attr)
        .and_then(|map| map.get(&article.id))
        .expect("resolved category list");
    assert_eq!(resolved_cat.len(), 1);
    assert_eq!(resolved_cat[0].id, category.id);

    let arts_attr = AttributeId::try_new("articles").expect("valid attr id");
    let cat_relations = instance_repo
        .fetch_relations(category_type_id, std::slice::from_ref(&arts_attr), &[category.id])
        .await
        .expect("fetch_relations category.articles");
    let resolved_arts = cat_relations
        .get(&arts_attr)
        .and_then(|map| map.get(&category.id))
        .expect("resolved articles list");
    assert_eq!(resolved_arts.len(), 1);
    assert_eq!(resolved_arts[0].id, article.id);
}

async fn assert_lifecycle_working_edits(
    instance_repo: &SqlxDocumentInstanceRepository,
    article: &mut DocumentInstance,
    article_type_id: &DocumentTypeId,
    author_actor: &ActorId,
) {
    let title_attr = AttributeId::try_new("title").expect("valid attr id");
    article.edit_draft(author_actor.clone(), Utc::now());
    article.set_field(
        title_attr.clone(),
        text_val("Hexagonal Architecture in Rust (Edition 2)"),
    );

    let modified_display = create_article_display_values(
        "Hexagonal Architecture in Rust (Edition 2)",
        "Engineering",
    );
    instance_repo
        .save_draft(
            article,
            &modified_display,
            "Hexagonal Architecture in Rust (Edition 2)",
        )
        .await
        .expect("save modified draft");

    let live_article = instance_repo
        .find_by_id(article_type_id, article.id, VersionView::Published)
        .await
        .expect("find live published")
        .expect("live row still exists");
    assert!(matches!(
        live_article.content.publication_state,
        PublicationState::Published { revision: 1, .. }
    ));
    assert_eq!(
        live_article.get_field(&title_attr),
        Some(&text_val("Hexagonal Architecture in Rust"))
    );

    let working_draft = instance_repo
        .find_by_id(article_type_id, article.id, VersionView::Working)
        .await
        .expect("find working draft")
        .expect("working draft exists");
    assert!(matches!(
        working_draft.content.publication_state,
        PublicationState::Modified {
            live_revision: 1,
            ..
        }
    ));

    let pub_page = instance_repo
        .find_published(article_type_id, Pagination::default(), vec![])
        .await
        .expect("find_published while modified");
    assert!(pub_page.items.iter().any(|item| item.id == article.id));

    let headers_page = instance_repo
        .list_admin_headers(article_type_id, Pagination::default(), None)
        .await
        .expect("list_admin_headers");
    let header = headers_page
        .items
        .iter()
        .find(|h| h.id == article.id)
        .expect("header found for article");
    assert_eq!(header.status, DocumentStatus::Modified);
}

async fn assert_lifecycle_publish_v2_and_snapshots(
    instance_repo: &SqlxDocumentInstanceRepository,
    snapshot_repo: &SqlxDocumentSnapshotRepository,
    article: &mut DocumentInstance,
    article_type_id: &DocumentTypeId,
    author_actor: &ActorId,
    rev1_snapshot: &DocumentSnapshot,
) {
    let rev2 = article
        .publish(author_actor.clone(), Utc::now())
        .expect("article.publish v2");
    assert_eq!(rev2, 2);

    let modified_display = create_article_display_values(
        "Hexagonal Architecture in Rust (Edition 2)",
        "Engineering",
    );
    instance_repo
        .publish(
            article,
            2,
            &modified_display,
            "Hexagonal Architecture in Rust (Edition 2)",
            Some(rev1_snapshot),
        )
        .await
        .expect("publish rev 2 with snapshot");

    let snapshots = snapshot_repo
        .list_snapshots(article_type_id, article.id)
        .await
        .expect("list_snapshots");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].revision, 1);

    let retrieved_snap = snapshot_repo
        .get_snapshot(article_type_id, article.id, 1)
        .await
        .expect("get_snapshot")
        .expect("snapshot exists");
    assert_eq!(retrieved_snap.revision, 1);
    assert_eq!(retrieved_snap.reason, SnapshotReason::Publish);
}

async fn assert_lifecycle_discard_and_unpublish(
    instance_repo: &SqlxDocumentInstanceRepository,
    article: &mut DocumentInstance,
    article_type_id: &DocumentTypeId,
    author_actor: &ActorId,
) {
    let modified_display = create_article_display_values(
        "Hexagonal Architecture in Rust (Edition 2)",
        "Engineering",
    );
    article.edit_draft(author_actor.clone(), Utc::now());
    instance_repo
        .save_draft(article, &modified_display, "Draft To Discard")
        .await
        .expect("save temp draft");

    let discarded = instance_repo
        .discard_draft(article_type_id, article.id)
        .await
        .expect("discard_draft")
        .expect("restored live instance");
    assert!(matches!(
        discarded.content.publication_state,
        PublicationState::Published { revision: 2, .. }
    ));

    let rev2_snapshot = DocumentSnapshot {
        id: SnapshotId::new(Uuid::now_v7()),
        document_type_id: article_type_id.clone(),
        document_id: article.id,
        revision: 2,
        fields: article.fields().clone(),
        relations: article.relations.clone(),
        created_at: Utc::now(),
        created_by: author_actor.clone(),
        reason: SnapshotReason::Unpublish,
    };
    article
        .unpublish(author_actor.clone(), Utc::now())
        .expect("article.unpublish");
    instance_repo
        .unpublish(
            article,
            &modified_display,
            "Hexagonal Architecture in Rust (Edition 2)",
            &rev2_snapshot,
        )
        .await
        .expect("unpublish");

    let pub_after = instance_repo
        .find_by_id(article_type_id, article.id, VersionView::Published)
        .await
        .expect("find_by_id published after unpublish");
    assert!(pub_after.is_none());

    let working_after = instance_repo
        .find_by_id(article_type_id, article.id, VersionView::Working)
        .await
        .expect("find_by_id working after unpublish")
        .expect("unpublished instance in drafts");
    assert!(matches!(
        working_after.content.publication_state,
        PublicationState::Unpublished {
            last_published_revision: 2,
            ..
        }
    ));
}

async fn assert_lifecycle_delete(
    instance_repo: &SqlxDocumentInstanceRepository,
    article_type_id: &DocumentTypeId,
    article_id: DocumentInstanceId,
) {
    instance_repo
        .delete(article_type_id, article_id)
        .await
        .expect("delete article");

    let final_working = instance_repo
        .find_by_id(article_type_id, article_id, VersionView::Working)
        .await
        .expect("find working after delete");
    assert!(final_working.is_none());

    let final_published = instance_repo
        .find_by_id(article_type_id, article_id, VersionView::Published)
        .await
        .expect("find published after delete");
    assert!(final_published.is_none());
}

// ---------------------------------------------------------------------------
// Offline Integration Tests (Always Running, No Database Required)
// ---------------------------------------------------------------------------

#[test]
fn test_schema_naming_precomputes_tables_columns_and_symmetric_relations() {
    let ctx = load_test_system_context();
    let naming = SchemaNaming::from_registry(&ctx.schema);

    let article_id = DocumentTypeId::try_new("article").expect("valid id");
    let category_id = DocumentTypeId::try_new("category").expect("valid id");
    let site_settings_id = DocumentTypeId::try_new("site-settings").expect("valid id");

    let article = naming
        .require(&article_id)
        .expect("article naming must exist");
    let category = naming
        .require(&category_id)
        .expect("category naming must exist");
    let site_settings = naming
        .require(&site_settings_id)
        .expect("site-settings naming must exist");

    assert_eq!(article.table_name, "articles");
    assert_eq!(category.table_name, "categories");
    assert_eq!(site_settings.table_name, "site_settings");

    for field_name in ["title", "slug", "views", "body"] {
        let attr_id = AttributeId::try_new(field_name).expect("valid attr id");
        assert!(
            article.field_column(&attr_id).is_some(),
            "expected precomputed column for article field '{field_name}'"
        );
    }

    assert!(!article.wide_upsert_columns.is_empty());
    assert!(!article.wide_update_columns.is_empty());

    let cat_rel_attr = AttributeId::try_new("category").expect("valid attr id");
    let art_rel_attr = AttributeId::try_new("articles").expect("valid attr id");

    let article_cat_rel = article
        .relation(&cat_rel_attr)
        .expect("article.category relation must exist");
    let category_art_rel = category
        .relation(&art_rel_attr)
        .expect("category.articles relation must exist");

    assert_eq!(
        article_cat_rel.junction_table_name,
        "articles_category_links"
    );
    assert_eq!(
        category_art_rel.junction_table_name,
        "articles_category_links"
    );
    assert_eq!(
        article_cat_rel.junction_table_name,
        category_art_rel.junction_table_name
    );
}

#[test]
fn test_query_compilation_for_loaded_schema() {
    let ctx = load_test_system_context();
    let naming = SchemaNaming::from_registry(&ctx.schema);
    let article_id = DocumentTypeId::try_new("article").expect("valid id");
    let article = naming
        .require(&article_id)
        .expect("article naming must exist");

    let select_stmt = select_by_id_query(article, Uuid::now_v7());
    let (select_sql, _) = compile_select(&select_stmt);
    assert!(select_sql.contains("SELECT"));
    assert!(select_sql.contains("\"articles\""));

    let pagination = Pagination {
        page: 1,
        page_size: 10,
    };
    let find_pub_stmt =
        find_published_query(article, &[], pagination).expect("compilation of find_published");
    let (pub_sql, _) = compile_select(&find_pub_stmt);
    let lower_pub_sql = pub_sql.to_lowercase();
    assert!(
        !lower_pub_sql.contains("has_pending_draft"),
        "find_published_query must not filter has_pending_draft per ADR-004 visitor visibility"
    );

    let headers_stmt = list_draft_enabled_headers_query(article, pagination, None);
    let (headers_sql, _) = compile_select(&headers_stmt);
    assert!(headers_sql.contains("UNION ALL"));
    let lower_headers_sql = headers_sql.to_lowercase();
    assert!(
        lower_headers_sql.contains("has_pending_draft"),
        "list_draft_enabled_headers_query must filter has_pending_draft in published branch"
    );

    let id1 = Uuid::now_v7();
    let id2 = Uuid::now_v7();
    let titles_stmt = find_display_titles_by_ids_query(article, &[id1, id2], true);
    let (titles_sql, _) = compile_select(&titles_stmt);
    assert!(titles_sql.contains("UNION ALL"));
    assert!(titles_sql.contains("\"display_title\""));

    let cat_rel_attr = AttributeId::try_new("category").expect("valid attr id");
    let cat_rel = article.relation(&cat_rel_attr).expect("category relation");
    let links_stmt = insert_links_batch_query(
        cat_rel.junction_table_iden,
        Uuid::now_v7(),
        &[DocumentInstanceId::new(Uuid::now_v7())],
    )
    .expect("returns insert links query");
    let (links_sql, _) = compile_insert(&links_stmt);
    assert!(links_sql.contains("INSERT INTO \"articles_category_links\""));
}

#[test]
fn test_codec_roundtrip_with_loaded_schema() {
    let ctx = load_test_system_context();
    let naming = SchemaNaming::from_registry(&ctx.schema);
    let article_id = DocumentTypeId::try_new("article").expect("valid id");
    let article = naming
        .require(&article_id)
        .expect("article naming must exist");

    let fields = create_test_fields_map();
    let json_val = serialize_fields_json(&fields).expect("serialize fields");
    let decoded_fields =
        deserialize_fields_json(json_val, article).expect("deserialize fields");
    assert_eq!(fields.len(), decoded_fields.len());

    let title_attr = AttributeId::try_new("title").expect("valid attr id");
    let views_attr = AttributeId::try_new("views").expect("valid attr id");
    assert_eq!(fields.get(&title_attr), decoded_fields.get(&title_attr));
    assert_eq!(fields.get(&views_attr), decoded_fields.get(&views_attr));

    let (cat_attr, cat_id, relations) = create_test_relations_map();
    let rel_json = serialize_relations_json(&relations).expect("serialize relations");
    let decoded_relations = deserialize_relations_json(rel_json).expect("deserialize relations");
    let decoded_target = decoded_relations.get(&cat_attr).expect("category targets");
    assert_eq!(decoded_target[0].target_instance_id, cat_id);
}

// ---------------------------------------------------------------------------
// Database-Backed Integration Tests (Gracefully Skip If DATABASE_URL Missing)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_role_user_assignment_and_access_request_repositories_live() {
    let Some((pool, _ctx, _naming)) = setup_isolated_db().await else {
        eprintln!("Skipping live database test: DATABASE_URL is not set");
        return;
    };

    let role_repo = SqlxRoleRepository::new(pool.clone());
    let assignment_repo = SqlxUserRoleAssignmentRepository::new(pool.clone());
    let access_repo = SqlxAccessRequestRepository::new(pool);

    let (admin_role, custom_role) = assert_roles_behavior(&role_repo).await;

    let admin_actor = ActorId::User(UserId::try_new("usr_system_admin").expect("valid user id"));
    assert_user_role_assignment_behavior(
        &assignment_repo,
        admin_role.id,
        custom_role.id,
        &admin_actor,
    )
    .await;

    assert_access_request_behavior(&access_repo, custom_role.id).await;
}

#[tokio::test]
async fn test_document_instance_and_snapshot_lifecycle_live() {
    let Some((pool, ctx, naming)) = setup_isolated_db().await else {
        eprintln!("Skipping live database test: DATABASE_URL is not set");
        return;
    };

    let instance_repo = SqlxDocumentInstanceRepository::new(pool.clone(), ctx, naming);
    let snapshot_repo = SqlxDocumentSnapshotRepository::new(pool, ctx, naming);

    let article_type_id = DocumentTypeId::try_new("article").expect("valid doc type id");
    let category_type_id = DocumentTypeId::try_new("category").expect("valid doc type id");
    let author_actor = ActorId::User(UserId::try_new("usr_author_01").expect("valid user id"));
    let now = Utc::now();

    let mut category = DocumentInstance::new(category_type_id.clone(), author_actor.clone(), now);
    let name_attr = AttributeId::try_new("name").expect("valid attr id");
    category.set_field(
        name_attr,
        text_val("Engineering"),
    );

    let mut article = DocumentInstance::new(article_type_id.clone(), author_actor.clone(), now);
    let title_attr = AttributeId::try_new("title").expect("valid attr id");
    let slug_attr = AttributeId::try_new("slug").expect("valid attr id");
    let cat_attr = AttributeId::try_new("category").expect("valid attr id");

    article.set_field(
        title_attr,
        text_val("Hexagonal Architecture in Rust"),
    );
    article.set_field(
        slug_attr,
        uid_val("hexagonal-architecture-in-rust"),
    );
    article.set_relations(cat_attr, vec![category.id]);

    assert_lifecycle_draft_creation(
        &instance_repo,
        &category,
        &article,
        &category_type_id,
        &article_type_id,
    )
    .await;

    assert_lifecycle_publish_and_relations(
        &instance_repo,
        &mut category,
        &mut article,
        &category_type_id,
        &article_type_id,
        &author_actor,
    )
    .await;

    let rev1_snapshot = DocumentSnapshot {
        id: SnapshotId::new(Uuid::now_v7()),
        document_type_id: article_type_id.clone(),
        document_id: article.id,
        revision: 1,
        fields: article.fields().clone(),
        relations: article.relations.clone(),
        created_at: Utc::now(),
        created_by: author_actor.clone(),
        reason: SnapshotReason::Publish,
    };

    assert_lifecycle_working_edits(
        &instance_repo,
        &mut article,
        &article_type_id,
        &author_actor,
    )
    .await;

    assert_lifecycle_publish_v2_and_snapshots(
        &instance_repo,
        &snapshot_repo,
        &mut article,
        &article_type_id,
        &author_actor,
        &rev1_snapshot,
    )
    .await;

    assert_lifecycle_discard_and_unpublish(
        &instance_repo,
        &mut article,
        &article_type_id,
        &author_actor,
    )
    .await;

    assert_lifecycle_delete(&instance_repo, &article_type_id, article.id).await;
}
