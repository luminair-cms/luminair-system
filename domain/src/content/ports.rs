use std::collections::HashMap;
use std::future::Future;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::header::{DocumentHeader, DocumentSnapshot, DocumentStatus, SnapshotHeader, VersionView};
use super::instance::{DocumentInstance, DocumentInstanceId};
use super::values::DomainValue;
use crate::errors::DomainError;
use crate::schema::{AttributeId, DocumentTypeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub page_size: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

/// Equality filter on a single scalar field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFilter {
    pub attribute_id: AttributeId,
    pub value: DomainValue,
}

/// A mapping of relation attribute to a map of parent instance IDs and their related instances:
/// AttributeId -> (ParentInstanceId -> Vec<RelatedInstance>)
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

    /// Visitor query: queries strictly the published wide table (or all instances in legacy/find_by_type).
    fn find_by_type(
        &self,
        type_id: &DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<Page<DocumentInstance>, DomainError>> + Send;

    /// Queries strictly published instances from the wide table.
    fn find_published(
        &self,
        type_id: &DocumentTypeId,
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

    /// Phase 2 batched lookup for singular relational display fields: returns `id -> display_title`.
    fn find_display_titles_by_ids(
        &self,
        type_id: &DocumentTypeId,
        ids: &[DocumentInstanceId],
    ) -> impl Future<Output = Result<HashMap<DocumentInstanceId, String>, DomainError>> + Send;

    fn count(
        &self,
        type_id: &DocumentTypeId,
        filters: Vec<FieldFilter>,
    ) -> impl Future<Output = Result<u64, DomainError>> + Send;

    /// Batch-loads relations for a set of parent instance IDs.
    fn fetch_relations(
        &self,
        type_id: &DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> impl Future<Output = Result<RelationMap, DomainError>> + Send;

    /// Persists a working draft (`Draft`, `Modified`, or `Unpublished`) along with materialized display identity.
    fn save_draft(
        &self,
        instance: &DocumentInstance,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Publishes the document instance to the wide table, clears any pending draft row,
    /// and records the publication snapshot atomically.
    fn publish(
        &self,
        instance: &DocumentInstance,
        revision: u32,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
        snapshot: &DocumentSnapshot,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Unpublishes the document instance from the wide table, preserves it in `document_drafts`
    /// with `Unpublished` state, and records the unpublish snapshot atomically.
    fn unpublish(
        &self,
        instance: &DocumentInstance,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
        snapshot: &DocumentSnapshot,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    /// Discards pending draft edits and restores the clean published row (`has_pending_draft = false`).
    fn discard_draft(
        &self,
        type_id: &DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<Option<DocumentInstance>, DomainError>> + Send;

    /// Deletes a document instance from both draft and published stores.
    fn delete(
        &self,
        type_id: &DocumentTypeId,
        id: DocumentInstanceId,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    fn exists_for_type(
        &self,
        type_id: &DocumentTypeId,
    ) -> impl Future<Output = Result<bool, DomainError>> + Send;
}

pub trait DocumentSnapshotRepository: Send + Sync {
    fn save_snapshot(
        &self,
        snapshot: &DocumentSnapshot,
    ) -> impl Future<Output = Result<(), DomainError>> + Send;

    fn list_snapshots(
        &self,
        type_id: &DocumentTypeId,
        doc_id: DocumentInstanceId,
    ) -> impl Future<Output = Result<Vec<SnapshotHeader>, DomainError>> + Send;

    fn get_snapshot(
        &self,
        type_id: &DocumentTypeId,
        doc_id: DocumentInstanceId,
        revision: u32,
    ) -> impl Future<Output = Result<Option<DocumentSnapshot>, DomainError>> + Send;
}
