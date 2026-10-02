use std::collections::HashMap;

use indexmap::IndexSet;

use crate::schema::{DocumentKind, DocumentType, DocumentTypeId};

#[derive(Debug, Clone, Default)]
pub struct SchemaRegistry {
    types: IndexSet<DocumentType>,
    by_name: HashMap<String, DocumentTypeId>,
}


impl SchemaRegistry {
    pub fn new(types: Vec<DocumentType>) -> Self {
        let types: IndexSet<DocumentType> = types.into_iter().collect();

        let mut by_name = HashMap::new();
        for dt in types.iter() {
            let api_id = match dt.kind {
                DocumentKind::SingleType => dt.info.singular_name.as_ref().to_string(),
                DocumentKind::Collection => dt.info.plural_name.as_ref().to_string(),
            };
            by_name.insert(api_id, dt.id.clone());
        }

        Self {
            types, by_name
        }
    }

    pub fn iterate(&self) -> impl Iterator<Item = &DocumentType> {
        self.types.iter()
    }

    pub fn get(&self, id: &DocumentTypeId) -> Option<&DocumentType> {
        self.types.get(id)
    }

    pub fn find_by_name(&self, plural_name: &str) -> Option<&DocumentType> {
        self.by_name
            .get(plural_name)
            .and_then(|id| self.types.get(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{test_doc_type_id, DocumentTypeBuilder};

    #[test]
    fn test_schema_registry_indexes_collection_by_plural_and_single_by_singular() {
        let coll = DocumentTypeBuilder::collection("article").build();
        let single = DocumentTypeBuilder::single("site-settings").build();
        let coll_id = coll.id.clone();
        let single_id = single.id.clone();

        let registry = SchemaRegistry::new(vec![coll, single]);

        // Collection is found by plural name
        assert_eq!(registry.find_by_name("articles").unwrap().id, coll_id);
        // SingleType is found by singular name
        assert_eq!(registry.find_by_name("site-settings").unwrap().id, single_id);

        // Found by ID
        assert_eq!(registry.get(&coll_id).unwrap().id, coll_id);
        assert_eq!(registry.get(&single_id).unwrap().id, single_id);

        // Non-existent lookups return None
        assert!(registry.find_by_name("non-existent").is_none());
        assert!(registry.get(&test_doc_type_id("missing")).is_none());
    }

    #[test]
    fn test_schema_registry_iterate_returns_all_types() {
        let t1 = DocumentTypeBuilder::collection("article").build();
        let t2 = DocumentTypeBuilder::collection("author").build();

        let registry = SchemaRegistry::new(vec![t1, t2]);
        let names: Vec<_> = registry.iterate().map(|t| t.id.as_ref()).collect();
        assert_eq!(names, vec!["article", "author"]);
    }
}
