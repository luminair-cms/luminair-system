//! In-memory thread-safe fake repositories for application testing.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use domain::auth::{
    AccessRequest, AccessRequestId, AccessRequestRepository, AccessRequestStatus, Role, RoleId,
    RoleRepository, UserId, UserRoleAssignment, UserRoleAssignmentId, UserRoleAssignmentRepository,
};
use domain::content::{
    ContentValue, DocumentInstance, DocumentInstanceId, DocumentInstanceRepository, FieldFilter,
    Page, Pagination, RelationMap,
};
use domain::errors::DomainError;
use domain::schema::{AttributeId, DocumentTypeId};

/// Maps a poisoned `RwLock` to the error a real repository reports for a backend failure.
fn lock_error<T>(_: std::sync::PoisonError<T>) -> DomainError {
    DomainError::Storage("in-memory store lock poisoned".into())
}

/// Returns `true` when `instance` satisfies **all** `filters` (logical AND).
///
/// A filter matches when the instance has a scalar field with the filter's attribute whose value
/// is equal to the filter value. A missing field, `Null` and `LocalizedText` never match.
/// An empty filter list matches every instance.
fn matches_filters(instance: &DocumentInstance, filters: &[FieldFilter]) -> bool {
    filters.iter().all(|f| {
        matches!(
            instance.content.fields.get(&f.attribute_id),
            Some(ContentValue::Scalar(v)) if *v == f.value
        )
    })
}

/// Thread-safe in-memory fake for `DocumentInstanceRepository`.
#[derive(Debug, Clone, Default)]
pub struct FakeDocumentInstanceRepository {
    pub instances: Arc<RwLock<HashMap<DocumentInstanceId, DocumentInstance>>>,
    pub relations: Arc<RwLock<RelationMap>>,
}

impl FakeDocumentInstanceRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_instance(self, instance: DocumentInstance) -> Self {
        self.instances
            .write()
            .expect("lock write")
            .insert(instance.id, instance);
        self
    }

    pub fn add_relation_data(
        &self,
        attr: AttributeId,
        parent_id: DocumentInstanceId,
        related: Vec<DocumentInstance>,
    ) {
        let mut map = self.relations.write().expect("lock write");
        let parent_map = map.entry(attr).or_default();
        parent_map.insert(parent_id, related);
    }
}

impl DocumentInstanceRepository for FakeDocumentInstanceRepository {
    async fn find_by_id(
        &self,
        _type_id: &'static DocumentTypeId,
        id: DocumentInstanceId,
    ) -> Result<Option<DocumentInstance>, DomainError> {
        let store = self.instances.read().map_err(lock_error)?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_type(
        &self,
        type_id: &'static DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> Result<Page<DocumentInstance>, DomainError> {
        let store = self.instances.read().map_err(lock_error)?;

        let mut filtered: Vec<DocumentInstance> = store
            .values()
            .filter(|inst| inst.document_type_id == *type_id && matches_filters(inst, &filters))
            .cloned()
            .collect();
        // Deterministic order (oldest first, id as tie-breaker): HashMap order is random.
        filtered.sort_by_key(|inst| (inst.audit.created_at, *inst.id.as_ref()));

        let total = filtered.len() as u64;
        let start = (u64::from(pagination.page.saturating_sub(1)) * u64::from(pagination.page_size))
            as usize;
        let items: Vec<DocumentInstance> = filtered
            .into_iter()
            .skip(start)
            .take(pagination.page_size as usize)
            .collect();

        Ok(Page {
            items,
            total,
            page: pagination.page,
            page_size: pagination.page_size,
        })
    }

    async fn count(
        &self,
        type_id: &'static DocumentTypeId,
        filters: Vec<FieldFilter>,
    ) -> Result<u64, DomainError> {
        let store = self.instances.read().map_err(lock_error)?;
        let count = store
            .values()
            .filter(|inst| inst.document_type_id == *type_id && matches_filters(inst, &filters))
            .count() as u64;
        Ok(count)
    }

    async fn fetch_relations(
        &self,
        _type_id: &'static DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> Result<RelationMap, DomainError> {
        let store = self.relations.read().map_err(lock_error)?;

        let mut result: RelationMap = HashMap::new();
        for attr in attributes {
            if let Some(by_parent) = store.get(attr) {
                let mut filtered_by_parent = HashMap::new();
                for pid in parent_ids {
                    if let Some(instances) = by_parent.get(pid) {
                        filtered_by_parent.insert(*pid, instances.clone());
                    }
                }
                result.insert(attr.clone(), filtered_by_parent);
            }
        }
        Ok(result)
    }

    async fn save(&self, instance: &DocumentInstance) -> Result<(), DomainError> {
        let mut store = self.instances.write().map_err(lock_error)?;
        store.insert(instance.id, instance.clone());

        let mut rel_store = self.relations.write().map_err(lock_error)?;

        for by_parent in rel_store.values_mut() {
            by_parent.remove(&instance.id);
        }

        for (attr, resolved_list) in &instance.relations {
            let by_parent = rel_store.entry(attr.clone()).or_default();
            let related_instances: Vec<DocumentInstance> = resolved_list
                .iter()
                .filter_map(|r| store.get(&r.target_instance_id).cloned())
                .collect();
            by_parent.insert(instance.id, related_instances);
        }

        Ok(())
    }

    async fn delete(
        &self,
        _type_id: &'static DocumentTypeId,
        id: DocumentInstanceId,
    ) -> Result<(), DomainError> {
        let mut store = self.instances.write().map_err(lock_error)?;
        store.remove(&id);

        let mut rel_store = self.relations.write().map_err(lock_error)?;
        for by_parent in rel_store.values_mut() {
            by_parent.remove(&id);
        }

        Ok(())
    }

    async fn exists_for_type(&self, type_id: &'static DocumentTypeId) -> Result<bool, DomainError> {
        let store = self.instances.read().map_err(lock_error)?;
        Ok(store.values().any(|inst| inst.document_type_id == *type_id))
    }
}

/// Thread-safe in-memory fake for `RoleRepository`.
#[derive(Debug, Clone, Default)]
pub struct FakeRoleRepository {
    pub roles: Arc<RwLock<HashMap<RoleId, Role>>>,
}

impl FakeRoleRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_role(self, role: Role) -> Self {
        self.roles
            .write()
            .expect("lock write")
            .insert(role.id, role);
        self
    }
}

impl RoleRepository for FakeRoleRepository {
    async fn find_by_id(&self, id: RoleId) -> Result<Option<Role>, DomainError> {
        let store = self.roles.read().map_err(lock_error)?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_ids(&self, ids: &[RoleId]) -> Result<Vec<Role>, DomainError> {
        let store = self.roles.read().map_err(lock_error)?;
        Ok(ids.iter().filter_map(|id| store.get(id).cloned()).collect())
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Role>, DomainError> {
        let store = self.roles.read().map_err(lock_error)?;
        Ok(store.values().find(|r| r.name == name).cloned())
    }

    async fn find_all(&self) -> Result<Vec<Role>, DomainError> {
        let store = self.roles.read().map_err(lock_error)?;
        Ok(store.values().cloned().collect())
    }

    async fn save(&self, role: &Role) -> Result<(), DomainError> {
        let mut store = self.roles.write().map_err(lock_error)?;
        store.insert(role.id, role.clone());
        Ok(())
    }
}

/// Thread-safe in-memory fake for `UserRoleAssignmentRepository`.
#[derive(Debug, Clone, Default)]
pub struct FakeUserRoleAssignmentRepository {
    pub assignments: Arc<RwLock<Vec<UserRoleAssignment>>>,
}

impl FakeUserRoleAssignmentRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl UserRoleAssignmentRepository for FakeUserRoleAssignmentRepository {
    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<UserRoleAssignment>, DomainError> {
        let store = self.assignments.read().map_err(lock_error)?;
        Ok(store
            .iter()
            .filter(|a| a.user_id == *user_id)
            .cloned()
            .collect())
    }

    async fn exists_admin(&self, admin_role_id: RoleId) -> Result<bool, DomainError> {
        let store = self.assignments.read().map_err(lock_error)?;
        Ok(store.iter().any(|a| a.role_id == admin_role_id))
    }

    async fn save(&self, assignment: &UserRoleAssignment) -> Result<(), DomainError> {
        let mut store = self.assignments.write().map_err(lock_error)?;
        store.retain(|a| !(a.user_id == assignment.user_id && a.role_id == assignment.role_id));
        store.push(assignment.clone());
        Ok(())
    }

    async fn save_all(&self, assignments: &[UserRoleAssignment]) -> Result<(), DomainError> {
        let mut store = self.assignments.write().map_err(lock_error)?;
        for assignment in assignments {
            store.retain(|a| !(a.user_id == assignment.user_id && a.role_id == assignment.role_id));
            store.push(assignment.clone());
        }
        Ok(())
    }

    async fn delete(&self, id: UserRoleAssignmentId) -> Result<(), DomainError> {
        let mut store = self.assignments.write().map_err(lock_error)?;
        store.retain(|a| a.id != id);
        Ok(())
    }
}

/// Thread-safe in-memory fake for `AccessRequestRepository`.
#[derive(Debug, Clone, Default)]
pub struct FakeAccessRequestRepository {
    pub requests: Arc<RwLock<HashMap<AccessRequestId, AccessRequest>>>,
}

impl FakeAccessRequestRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AccessRequestRepository for FakeAccessRequestRepository {
    async fn find_by_id(&self, id: AccessRequestId) -> Result<Option<AccessRequest>, DomainError> {
        let store = self.requests.read().map_err(lock_error)?;
        Ok(store.get(&id).cloned())
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Option<AccessRequest>, DomainError> {
        let store = self.requests.read().map_err(lock_error)?;
        Ok(store
            .values()
            .filter(|r| r.user_id == *user_id)
            .max_by_key(|r| r.requested_at)
            .cloned())
    }

    async fn find_active_by_user(
        &self,
        user_id: &UserId,
    ) -> Result<Option<AccessRequest>, DomainError> {
        let store = self.requests.read().map_err(lock_error)?;
        Ok(store
            .values()
            .find(|r| r.user_id == *user_id && r.is_active())
            .cloned())
    }

    async fn find_pending(&self) -> Result<Vec<AccessRequest>, DomainError> {
        let store = self.requests.read().map_err(lock_error)?;
        Ok(store
            .values()
            .filter(|r| matches!(r.status, AccessRequestStatus::Pending))
            .cloned()
            .collect())
    }

    async fn save(&self, request: &AccessRequest) -> Result<(), DomainError> {
        let mut store = self.requests.write().map_err(lock_error)?;
        store.insert(request.id, request.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Self-tests for the fakes: a fake that silently diverges from the repository contract makes
    //! every use-case spec built on it meaningless.

    use std::panic::{AssertUnwindSafe, catch_unwind};

    use chrono::{Duration, Utc};
    use domain::content::{DomainValue, PrimitiveValue};
    use domain::test_support::{test_attr_id, test_doc_type_id, test_user_id};

    use super::*;

    fn leak_type(name: &str) -> &'static DocumentTypeId {
        Box::leak(Box::new(test_doc_type_id(name)))
    }

    fn text(v: &str) -> DomainValue {
        DomainValue::Primitive(PrimitiveValue::Text(v.into()))
    }

    fn instance_with(
        type_id: &'static DocumentTypeId,
        fields: &[(&str, ContentValue)],
    ) -> DocumentInstance {
        let mut inst = DocumentInstance::new(type_id.clone(), None, Utc::now());
        for (name, value) in fields {
            inst.content
                .fields
                .insert(test_attr_id(name), value.clone());
        }
        inst
    }

    fn filter(attr: &str, value: DomainValue) -> FieldFilter {
        FieldFilter {
            attribute_id: test_attr_id(attr),
            value,
        }
    }

    fn all() -> Pagination {
        Pagination {
            page: 1,
            page_size: 100,
        }
    }

    fn seeded() -> (FakeDocumentInstanceRepository, &'static DocumentTypeId) {
        let t = leak_type("article");
        let repo = FakeDocumentInstanceRepository::new()
            .with_instance(instance_with(
                t,
                &[
                    ("title", ContentValue::Scalar(text("a"))),
                    ("lang", ContentValue::Scalar(text("en"))),
                ],
            ))
            .with_instance(instance_with(
                t,
                &[
                    ("title", ContentValue::Scalar(text("a"))),
                    ("lang", ContentValue::Scalar(text("uk"))),
                ],
            ))
            .with_instance(instance_with(
                t,
                &[("title", ContentValue::Scalar(text("b")))],
            ))
            .with_instance(instance_with(t, &[("title", ContentValue::Null)]));
        (repo, t)
    }

    #[tokio::test]
    async fn test_find_by_type_without_filters_returns_all_of_type() {
        let (repo, t) = seeded();
        let other = leak_type("other");
        repo.save(&instance_with(other, &[])).await.unwrap();

        let page = repo.find_by_type(t, all(), vec![]).await.unwrap();
        assert_eq!(page.total, 4);
        assert_eq!(repo.count(t, vec![]).await.unwrap(), 4);
    }

    #[tokio::test]
    async fn test_filter_matches_equal_scalar_only() {
        let (repo, t) = seeded();
        let f = vec![filter("title", text("a"))];

        assert_eq!(repo.count(t, f.clone()).await.unwrap(), 2);
        let page = repo.find_by_type(t, all(), f).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items.len(), 2);
    }

    #[tokio::test]
    async fn test_filters_are_combined_with_and() {
        let (repo, t) = seeded();
        let f = vec![filter("title", text("a")), filter("lang", text("uk"))];
        assert_eq!(repo.count(t, f).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn test_filter_never_matches_missing_field_or_null() {
        let (repo, t) = seeded();
        // `lang` is missing on two instances, `title` is Null on one.
        assert_eq!(
            repo.count(t, vec![filter("lang", text("de"))])
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            repo.count(t, vec![filter("absent", text("a"))])
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn test_pagination_total_counts_filtered_items_and_pages_are_stable() {
        let t = leak_type("article");
        let repo = FakeDocumentInstanceRepository::new();
        let base = Utc::now();
        let mut ids = Vec::new();
        for i in 0..5 {
            let mut inst = instance_with(t, &[("title", ContentValue::Scalar(text("x")))]);
            inst.audit.created_at = base + Duration::seconds(i);
            ids.push(inst.id);
            repo.save(&inst).await.unwrap();
        }
        let f = vec![filter("title", text("x"))];

        let p1 = repo
            .find_by_type(
                t,
                Pagination {
                    page: 1,
                    page_size: 2,
                },
                f.clone(),
            )
            .await
            .unwrap();
        let p3 = repo
            .find_by_type(
                t,
                Pagination {
                    page: 3,
                    page_size: 2,
                },
                f,
            )
            .await
            .unwrap();

        assert_eq!(p1.total, 5);
        assert_eq!(p1.items.iter().map(|i| i.id).collect::<Vec<_>>(), ids[0..2]);
        assert_eq!(p3.items.iter().map(|i| i.id).collect::<Vec<_>>(), ids[4..5]);
    }

    #[tokio::test]
    async fn test_poisoned_lock_is_reported_as_storage_error() {
        let repo = FakeDocumentInstanceRepository::new();
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = repo.instances.write().unwrap();
            panic!("poison the lock");
        }));

        let err = repo
            .exists_for_type(leak_type("article"))
            .await
            .unwrap_err();
        assert!(matches!(err, DomainError::Storage(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn test_poisoned_lock_in_other_fakes_is_storage_error() {
        let repo = FakeRoleRepository::new();
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = repo.roles.write().unwrap();
            panic!("poison the lock");
        }));
        let err = repo.find_all().await.unwrap_err();
        assert!(matches!(err, DomainError::Storage(_)), "got {err:?}");

        let repo = FakeUserRoleAssignmentRepository::new();
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = repo.assignments.write().unwrap();
            panic!("poison the lock");
        }));
        let err = repo.find_by_user(&test_user_id("u")).await.unwrap_err();
        assert!(matches!(err, DomainError::Storage(_)), "got {err:?}");

        let repo = FakeAccessRequestRepository::new();
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _guard = repo.requests.write().unwrap();
            panic!("poison the lock");
        }));
        let err = repo.find_pending().await.unwrap_err();
        assert!(matches!(err, DomainError::Storage(_)), "got {err:?}");
    }
}
