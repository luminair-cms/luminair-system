//! SystemContext combines declarative schema definitions and system-level configuration.

use crate::schema::{DocumentType, DocumentTypeId, SchemaRegistry};
use crate::system::{LocaleId, SystemConfig};

/// Consolidated immutable system context combining schema registry and system configuration.
#[derive(Debug, Clone)]
pub struct SystemContext {
    pub schema: SchemaRegistry,
    pub config: SystemConfig,
}

impl SystemContext {
    /// Creates a new `SystemContext` from a loaded schema registry and system configuration.
    pub fn new(schema: SchemaRegistry, config: SystemConfig) -> Self {
        Self { schema, config }
    }

    /// Looks up a document type by its unique identifier.
    pub fn find_type(&self, id: &DocumentTypeId) -> Option<&DocumentType> {
        self.schema.get(id)
    }

    /// Looks up a document type by its collection name (plural) or single-type name (singular).
    pub fn find_type_by_name(&self, name: &str) -> Option<&DocumentType> {
        self.schema.find_by_name(name)
    }

    pub fn all_types(&self) -> impl Iterator<Item = &DocumentType> {
        self.schema.iterate()
    }

    /// Returns the system default locale.
    pub fn default_locale(&self) -> &LocaleId {
        &self.config.default_locale
    }

    /// Checks if a locale is supported by the system.
    pub fn contains_locale(&self, locale: &LocaleId) -> bool {
        self.config.contains_locale(locale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        fixture_system_config, test_doc_type_id, test_locales, DocumentTypeBuilder,
    };

    #[test]
    fn test_system_context_delegation() {
        let type_id = test_doc_type_id("article");
        let doc_type = DocumentTypeBuilder::collection("article").build();
        let registry = SchemaRegistry::new(vec![doc_type]);
        let config = fixture_system_config();
        let (en, uk, fr) = test_locales();

        let ctx = SystemContext::new(registry, config);

        // Schema delegation
        assert_eq!(ctx.find_type(&type_id).unwrap().id, type_id);
        assert_eq!(ctx.find_type_by_name("articles").unwrap().id, type_id);
        assert_eq!(ctx.all_types().count(), 1);

        // Config delegation
        assert_eq!(ctx.default_locale(), &en);
        assert!(ctx.contains_locale(&uk));
        assert!(!ctx.contains_locale(&fr));
    }
}
