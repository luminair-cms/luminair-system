//! Integration tests for schema loading and `SystemContext` assembly.

use std::fs;

use domain::schema::DocumentTypeId;
use domain::system::LocaleId;
use infrastructure::schema_loader::{
    SchemaLoaderError, load_schemas_from_dir, load_system_context_from_dir,
};

fn create_temp_schema_dir(prefix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("luminair_test_{prefix}_{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(dir.join("document-types")).expect("create temp dir");
    dir
}

#[test]
fn test_load_schemas_from_dir_success() {
    let dir = create_temp_schema_dir("success");

    let system_config_json = r#"{
        "defaultLocale": "en",
        "availableLocales": ["en", "uk", "fr"],
        "instanceName": "Luminair Test"
    }"#;
    fs::write(dir.join("system-config.json"), system_config_json).expect("write system-config");

    let article_json = r#"{
        "kind": "collection",
        "info": {
            "singularName": "article",
            "pluralName": "articles",
            "displayName": "Blog Article",
            "description": "Company articles and announcements",
            "displayFields": ["title", "category"]
        },
        "options": {
            "draftAndPublish": true
        },
        "attributes": {
            "title": {
                "type": "text",
                "required": true,
                "constraints": [
                    { "minLength": 5 },
                    { "maxLength": 100 }
                ]
            },
            "summary": {
                "type": "localizedText",
                "required": true,
                "constraints": [
                    { "minLength": 10 }
                ]
            },
            "priority": {
                "type": { "integer": "int32" },
                "required": true,
                "unique": true,
                "constraints": [
                    { "min": 1 },
                    { "max": 10 }
                ]
            },
            "price": {
                "type": { "decimal": { "precision": 10, "scale": 2 } },
                "required": false
            },
            "category": {
                "relation": "hasOne",
                "target": "category"
            },
            "tags": {
                "relation": "hasMany",
                "target": "tag"
            }
        }
    }"#;
    fs::write(
        dir.join("document-types").join("article.json"),
        article_json,
    )
    .expect("write article.json");

    let category_json = r#"{
        "kind": "collection",
        "info": {
            "singularName": "category",
            "pluralName": "categories",
            "displayName": "Article Category",
            "displayFields": ["name"]
        },
        "options": {
            "draftAndPublish": false
        },
        "attributes": {
            "name": {
                "type": "text",
                "required": true,
                "unique": true,
                "constraints": [
                    { "minLength": 2 },
                    { "maxLength": 50 }
                ]
            },
            "articles": {
                "relation": "belongsToMany",
                "target": "article"
            }
        }
    }"#;
    fs::write(
        dir.join("document-types").join("category.json"),
        category_json,
    )
    .expect("write category.json");

    let tag_json = r#"{
        "kind": "collection",
        "info": {
            "singularName": "tag",
            "pluralName": "tags",
            "displayName": "Tag",
            "displayFields": ["name"]
        },
        "attributes": {
            "name": {
                "type": "text",
                "required": true,
                "unique": true
            }
        }
    }"#;
    fs::write(dir.join("document-types").join("tag.json"), tag_json).expect("write tag.json");

    // Test load_schemas_from_dir
    let (config, registry) = load_schemas_from_dir(&dir).expect("schemas load successfully");

    assert_eq!(config.default_locale.as_ref(), "en");
    assert!(config.contains_locale(&LocaleId::try_new("uk").unwrap()));

    let article_id = DocumentTypeId::try_new("article").unwrap();
    let article_type = registry.get(&article_id).expect("article type found");
    assert_eq!(article_type.fields.len(), 4);
    assert_eq!(article_type.relations.len(), 2);
    assert_eq!(article_type.info.display_fields.len(), 2);
    assert!(article_type.options.draft_and_publish);

    let category_id = DocumentTypeId::try_new("category").unwrap();
    let category_type = registry.get(&category_id).expect("category type found");
    assert_eq!(category_type.fields.len(), 1);
    assert_eq!(category_type.relations.len(), 1);
    assert!(!category_type.options.draft_and_publish);

    // Test load_system_context_from_dir
    let ctx = load_system_context_from_dir(&dir).expect("system context loaded");
    assert_eq!(ctx.default_locale().as_ref(), "en");
    assert!(ctx.find_type(&article_id).is_some());
    assert!(ctx.find_type(&category_id).is_some());

    // Clean up
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_load_schemas_missing_system_config() {
    let dir = create_temp_schema_dir("missing_config");
    let res = load_schemas_from_dir(&dir);

    assert!(matches!(
        res,
        Err(SchemaLoaderError::MissingSystemConfig { .. })
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_load_schemas_missing_document_types_dir() {
    let dir = std::env::temp_dir().join(format!("luminair_test_no_types_{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("system-config.json"),
        r#"{"defaultLocale":"en","availableLocales":["en"]}"#,
    )
    .unwrap();

    let res = load_schemas_from_dir(&dir);
    assert!(matches!(
        res,
        Err(SchemaLoaderError::MissingDocumentTypesDir { .. })
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_load_schemas_file_stem_mismatch() {
    let dir = create_temp_schema_dir("mismatch");
    fs::write(
        dir.join("system-config.json"),
        r#"{"defaultLocale":"en","availableLocales":["en"]}"#,
    )
    .unwrap();

    let json = r#"{
        "kind": "collection",
        "info": {
            "singularName": "post",
            "pluralName": "posts",
            "displayName": "Post"
        },
        "attributes": {}
    }"#;
    fs::write(dir.join("document-types").join("article.json"), json).unwrap();

    let res = load_schemas_from_dir(&dir);
    assert!(matches!(
        res,
        Err(SchemaLoaderError::FileStemMismatch { .. })
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_load_schemas_unregistered_relation_target() {
    let dir = create_temp_schema_dir("unregistered_target");
    fs::write(
        dir.join("system-config.json"),
        r#"{"defaultLocale":"en","availableLocales":["en"]}"#,
    )
    .unwrap();

    let json = r#"{
        "kind": "collection",
        "info": {
            "singularName": "article",
            "pluralName": "articles",
            "displayName": "Article"
        },
        "attributes": {
            "author": {
                "relation": "hasOne",
                "target": "author"
            }
        }
    }"#;
    fs::write(dir.join("document-types").join("article.json"), json).unwrap();

    let res = load_schemas_from_dir(&dir);
    assert!(matches!(
        res,
        Err(SchemaLoaderError::Domain(domain::errors::DomainError::DocumentTypeNotFound(id))) if id.as_ref() == "author"
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_load_schemas_invalid_display_fields_max_exceeded() {
    let dir = create_temp_schema_dir("max_display_fields");
    fs::write(
        dir.join("system-config.json"),
        r#"{"defaultLocale":"en","availableLocales":["en"]}"#,
    )
    .unwrap();

    let json = r#"{
        "kind": "collection",
        "info": {
            "singularName": "article",
            "pluralName": "articles",
            "displayName": "Article",
            "displayFields": ["f1", "f2", "f3", "f4"]
        },
        "attributes": {
            "f1": { "type": "text" },
            "f2": { "type": "text" },
            "f3": { "type": "text" },
            "f4": { "type": "text" }
        }
    }"#;
    fs::write(dir.join("document-types").join("article.json"), json).unwrap();

    let res = load_schemas_from_dir(&dir);
    assert!(matches!(
        res,
        Err(SchemaLoaderError::Domain(
            domain::errors::DomainError::InvalidSchemaDisplayField { .. }
        ))
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn test_load_schemas_reserved_sql_keyword_rejected() {
    let dir = create_temp_schema_dir("reserved_keyword");
    fs::write(
        dir.join("system-config.json"),
        r#"{"defaultLocale":"en","availableLocales":["en"]}"#,
    )
    .unwrap();

    let json = r#"{
        "kind": "collection",
        "info": {
            "singularName": "user",
            "pluralName": "users",
            "displayName": "User"
        },
        "attributes": {}
    }"#;
    fs::write(dir.join("document-types").join("user.json"), json).unwrap();

    let res = load_schemas_from_dir(&dir);
    assert!(matches!(
        res,
        Err(SchemaLoaderError::ReservedKeyword { keyword, .. }) if keyword == "user"
    ));
    let _ = fs::remove_dir_all(dir);
}
