//! Execution mode worker for configuration and schema validation (`test` mode).

use domain::schema::{DocumentKind, DocumentType};
use domain::system::SystemContext;

use super::config::CliConfig;
use super::report::{ConfigTestReport, DocumentTypeSummary};
use crate::persistence::naming::table_name_for_document_type;

// ---------------------------------------------------------------------------
// Public Mode Execution Entry Point
// ---------------------------------------------------------------------------

/// Executes the offline schema validation and configuration test mode.
///
/// Inspects loaded document types, available locales, and system configuration,
/// returning a deterministic [`ConfigTestReport`].
#[must_use]
pub(super) fn execute_test_mode(
    config: &CliConfig,
    context: &'static SystemContext,
) -> ConfigTestReport {
    let document_types = extract_document_summaries(context);
    let available_locales = extract_available_locales(context);

    ConfigTestReport {
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
    let mut summaries: Vec<DocumentTypeSummary> = context
        .all_types()
        .map(map_document_summary)
        .collect();
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
    context
        .config
        .available_locales
        .iter()
        .map(|l| l.as_ref().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use domain::common::DisplayName;
    use domain::schema::{
        AttributeId, DocumentKind, DocumentType, DocumentTypeId, DocumentTypeInfo,
        DocumentTypeOptions, FieldDefinition, FieldType, IntegerSize, PrimitiveType,
        SchemaRegistry,
    };
    use domain::system::{LocaleId, SystemConfig, SystemConfigId, SystemContext};

    use super::*;

    fn build_test_doc_type(id: &str, kind: DocumentKind, field_count: usize) -> DocumentType {
        let type_id = DocumentTypeId::try_new(id).unwrap();
        let plural_id = DocumentTypeId::try_new(format!("{id}s")).unwrap();

        let fields = (0..field_count)
            .map(|i| FieldDefinition {
                id: AttributeId::try_new(format!("field-{i}")).unwrap(),
                field_type: FieldType::Primitive(PrimitiveType::Integer(IntegerSize::I32)),
                unique: false,
                required: false,
                constraints: Default::default(),
            })
            .collect();

        DocumentType {
            id: type_id.clone(),
            kind,
            info: DocumentTypeInfo {
                title: DisplayName::try_new("Test").unwrap(),
                singular_name: type_id,
                plural_name: plural_id,
                description: None,
                display_fields: vec![],
            },
            options: DocumentTypeOptions {
                draft_and_publish: true,
            },
            fields,
            relations: Default::default(),
        }
    }

    fn sample_system_context() -> &'static SystemContext {
        let article = build_test_doc_type("article", DocumentKind::Collection, 3);
        let settings = build_test_doc_type("site-setting", DocumentKind::Singleton, 1);
        let registry = SchemaRegistry::new(vec![article, settings]);

        let locales = vec![
            LocaleId::try_new("en").unwrap(),
            LocaleId::try_new("uk").unwrap(),
        ];
        let default_locale = LocaleId::try_new("en").unwrap();
        let config =
            SystemConfig::new(SystemConfigId::new(uuid::Uuid::now_v7()), locales, default_locale)
                .unwrap();

        Box::leak(Box::new(SystemContext::new(registry, config)))
    }

    #[test]
    fn test_execute_test_mode_success() {
        let context = sample_system_context();
        let config = CliConfig {
            schema_dir: PathBuf::from("./test/schemas"),
            database_url: Some("postgres://localhost/luminair".to_string()),
            ..CliConfig::default()
        };

        let report = execute_test_mode(&config, context);

        assert_eq!(report.schema_dir, "./test/schemas");
        assert_eq!(report.default_locale, "en");
        assert_eq!(report.available_locales, vec!["en", "uk"]);
        assert!(report.database_url_configured);
        assert_eq!(report.document_types.len(), 2);

        // Deterministically sorted: "article", then "site-setting"
        assert_eq!(report.document_types[0].id, "article");
        assert_eq!(report.document_types[0].kind, "collection");
        assert_eq!(report.document_types[0].table_name, "articles");
        assert_eq!(report.document_types[0].field_count, 3);
        assert_eq!(report.document_types[0].relation_count, 0);

        assert_eq!(report.document_types[1].id, "site-setting");
        assert_eq!(report.document_types[1].kind, "singleton");
        assert_eq!(report.document_types[1].table_name, "site_setting");
        assert_eq!(report.document_types[1].field_count, 1);
    }

    #[test]
    fn test_execute_test_mode_database_url_unconfigured() {
        let context = sample_system_context();
        let config = CliConfig {
            schema_dir: PathBuf::from("./config/schemas"),
            database_url: None,
            ..CliConfig::default()
        };

        let report = execute_test_mode(&config, context);
        assert!(!report.database_url_configured);
    }
}
