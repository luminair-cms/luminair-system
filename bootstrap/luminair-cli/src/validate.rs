//! Execution mode worker for configuration and schema validation (`validate` mode).

use domain::schema::{DocumentKind, DocumentType};
use domain::system::SystemContext;

use super::config::CliConfig;
use super::report::{DocumentTypeSummary, ValidationReport};
use luminair_persistence::naming::table_name_for_document_type;

// ---------------------------------------------------------------------------
// Public Mode Execution Entry Point
// ---------------------------------------------------------------------------

/// Executes the offline schema validation and configuration validate mode.
///
/// Inspects loaded document types, available locales, and system configuration,
/// returning a deterministic [`ValidationReport`].
#[must_use]
pub(super) fn execute_validate_mode(
    config: &CliConfig,
    context: &'static SystemContext,
) -> ValidationReport {
    let document_types = extract_document_summaries(context);
    let available_locales = extract_available_locales(context);

    ValidationReport {
        schema_dir: config.schema_dir.display().to_string(),
        default_locale: context.default_locale().as_ref().to_string(),
        available_locales,
        document_types,
        database_url_configured: config.database_url.is_some(),
    }
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

fn extract_document_summaries(context: &SystemContext) -> Vec<DocumentTypeSummary> {
    let mut summaries: Vec<DocumentTypeSummary> =
        context.all_types().map(map_document_summary).collect();
    summaries.sort_by(|a, b| a.id.cmp(&b.id));
    summaries
}

fn map_document_summary(doc: &DocumentType) -> DocumentTypeSummary {
    DocumentTypeSummary {
        id: doc.id.to_string(),
        kind: map_document_kind(doc.kind),
        table_name: table_name_for_document_type(doc),
        draft_and_publish: doc.options.draft_and_publish,
        field_count: doc.fields.len(),
        relation_count: doc.relations.len(),
    }
}

fn map_document_kind(kind: DocumentKind) -> String {
    match kind {
        DocumentKind::Collection => "collection".to_string(),
        DocumentKind::Singleton => "singleton".to_string(),
    }
}

fn extract_available_locales(context: &SystemContext) -> Vec<String> {
    let mut locales: Vec<String> = context
        .config
        .available_locales
        .iter()
        .map(|loc| loc.as_ref().to_string())
        .collect();
    locales.sort();
    locales
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn create_test_system_context() -> &'static SystemContext {
        let temp_dir =
            std::env::temp_dir().join(format!("luminair_val_test_{}", uuid::Uuid::now_v7()));
        let doc_types_dir = temp_dir.join("document-types");
        std::fs::create_dir_all(&doc_types_dir).unwrap();

        let system_config_json = r#"{
            "defaultLocale": "en",
            "availableLocales": ["en", "uk", "fr"]
        }"#;
        std::fs::write(temp_dir.join("system-config.json"), system_config_json).unwrap();

        let article_json = r#"{
            "kind": "collection",
            "info": {
                "singularName": "article",
                "pluralName": "articles",
                "displayName": "Article"
            },
            "options": {
                "draftAndPublish": true
            },
            "attributes": {
                "title": {
                    "type": "text",
                    "required": true
                }
            }
        }"#;
        std::fs::write(doc_types_dir.join("article.json"), article_json).unwrap();

        let context = luminair_schema_loader::load_system_context_from_dir(&temp_dir).unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
        context
    }

    #[test]
    fn test_execute_validate_mode() {
        let context = create_test_system_context();
        let config = CliConfig {
            schema_dir: PathBuf::from("./config/schemas"),
            database_url: Some("postgres://localhost/test".to_string()),
            ..CliConfig::default()
        };

        let report = execute_validate_mode(&config, context);
        assert_eq!(report.default_locale, "en");
        assert_eq!(report.available_locales, vec!["en", "fr", "uk"]);
        assert!(report.database_url_configured);
        assert_eq!(report.document_types.len(), 1);
        assert_eq!(report.document_types[0].id, "article");
        assert_eq!(report.document_types[0].kind, "collection");
        assert_eq!(report.document_types[0].table_name, "articles");
        assert!(report.document_types[0].draft_and_publish);
        assert_eq!(report.document_types[0].field_count, 1);
    }
}
