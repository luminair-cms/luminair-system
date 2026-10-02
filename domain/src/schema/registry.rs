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

    pub fn find_by_name(&self, plural_name: &str)-> Option<&DocumentType> {
        self.by_name
        .get(plural_name)
        .and_then(|id| self.types.get(id))
    }
}
