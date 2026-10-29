use std::collections::HashMap;
use std::future::Future;

use indexmap::IndexMap;

use super::header::{
    DocumentHeader, DocumentSnapshot, DocumentStatus, SnapshotHeader, VersionView,
};
use super::instance::{DocumentInstance, DocumentInstanceId};
use super::values::DomainValue;
use crate::errors::DomainError;
use crate::schema::{AttributeId, DocumentTypeId};

pub use crate::common::{Page, Pagination};

/// Equality filter on a single scalar field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFilter {
    pub attribute_id: AttributeId,
    pub value: DomainValue,
}

/// A mapping of relation attribute to a map of parent instance IDs and their related instances:
/// `AttributeId` -> (`ParentInstanceId` -> `Vec<RelatedInstance>`)
pub type RelationMap = HashMap<AttributeId, HashMap<DocumentInstanceId, Vec<DocumentInstance>>>;

pub trait DocumentInstanceRepository: Send + Sync {
    /// Loads a single document instance by its type, ID, and requested lifecycle view:
    /// - `VersionView::Published`: strictly loads the live published row from the wide table (`[type]`).
    /// - `VersionView::Working`: loads the working copy from `document_drafts` if present, otherwise falls back to `[type]`.
    fn find_by_id(
        &self,
        type_id: &DocumentTypeId,
        id: DocumentInstanceId,
        view: VersionView,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, DomainError>> + Send;

    /// Queries document instances by document type, requested lifecycle view (`VersionView`),
    /// pagination, and equality field filters:
    /// - `VersionView::Published`: queries strictly published instances from the wide table.
    /// - `VersionView::Working`: queries working copies (drafts or working state).
    fn find_by_type(
        &self,
        type_id: &DocumentTypeId,
        view: VersionView,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<Page<DocumentInstance>, DomainError>> + Send;

    /// Admin Studio listing: returns lightweight `DocumentHeader` projections across drafts and published rows.
    fn list_admin_headers(
        &self,
        type_id: &DocumentTypeId,
        pagination: Pagination,
        status_filter: Option<DocumentStatus>,
    ) -> impl Future<Output = Result<Page<DocumentHeader>, DomainError>> + Send;

    /// Resolves human-readable display titles for a list of document instance IDs within a target type.
    fn find_display_titles_by_ids(
        &self,
        type_id: &DocumentTypeId,
        ids: &[DocumentInstanceId],
    ) -> impl Future<Output = Result<HashMap<DocumentInstanceId, String>, DomainError>> + Send;

    /// Checks if at least one document instance exists for the given document type.
    fn exists_for_type(
        &self,
        type_id: &DocumentTypeId,
    ) -> impl Future<Output = Result<bool, DomainError>> + Send;

    /// Returns the total number of document instances for the given type and filters.
    fn count(
        &self,
        type_id: &DocumentTypeId,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<u64, DomainError>> + Send;

    /// Fetches related document instances for the given parent IDs and relation attributes.
    fn fetch_relations(
        &self,
        type_id: &DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> impl Future<Output = Result<RelationMap, DomainError>> + Send;

    /// Persists or updates a work-in-progress draft in `document_drafts`.
    fn save_draft(
        &self,
        instance: &DocumentInstance,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Atomically publishes a document instance:
    /// 1. Upserts the wide table row (`[type]`) with the live revision.
    /// 2. Synchronizes relational links in junction tables (`[type]_[attr]_links`).
    /// 3. Updates `document_drafts` to clear working status (`has_pending_draft = false`).
    /// 4. Archives the superseded live revision (if provided) into `document_snapshots`.
    fn publish(
        &self,
        instance: &DocumentInstance,
        revision: u32,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
        archived_snapshot: Option<&DocumentSnapshot>,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Atomically unpublishes a live document:
    /// 1. Removes the live row from the wide table (`[type]`) and cascades junction links.
    /// 2. Updates `document_drafts` status to `unpublished`.
    /// 3. Appends an unpublish snapshot to `document_snapshots`.
    fn unpublish(
        &self,
        instance: &DocumentInstance,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
        snapshot: &DocumentSnapshot,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Discards uncommitted draft changes, restoring the clean published state or deleting the draft.
    fn discard_draft(
        &self,
        type_id: &DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, DomainError>> + Send;

    /// Permanently deletes a document instance from both `document_drafts` and the wide table.
    fn delete(
        &self,
        type_id: &DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;
}

pub trait DocumentSnapshotRepository: Send + Sync {
    /// Appends an immutable historical revision snapshot to `document_snapshots`.
    fn save_snapshot(
        &self,
        snapshot: &DocumentSnapshot,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Lists lightweight snapshot revision headers for a document, ordered newest first.
    fn list_snapshots(
        &self,
        type_id: &DocumentTypeId,
        document_id: DocumentInstanceId,
    ) -> impl Future<Output = Result<Vec<SnapshotHeader>, DomainError>> + Send;

    /// Retrieves a specific historical snapshot by its revision number.
    fn get_snapshot(
        &self,
        type_id: &DocumentTypeId,
        document_id: DocumentInstanceId,
        revision: u32,
    ) -> impl Future<Output = Result<Option<DocumentSnapshot>, DomainError>> + Send;
}
