//! SQLx implementation of `DocumentInstanceRepository` and `DocumentSnapshotRepository`.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use sqlx::postgres::PgRow;
use sqlx::{AssertSqlSafe, PgConnection, PgPool, Row};
use uuid::Uuid;

use domain::auth::ActorId;
use domain::content::header::{
    DocumentHeader, DocumentSnapshot, DocumentStatus, SnapshotHeader, VersionView,
};
use domain::content::instance::{DocumentInstance, DocumentInstanceId, PublicationState};
use domain::content::ports::{
    DocumentInstanceRepository, DocumentSnapshotRepository, FieldFilter, Page, Pagination,
    RelationMap,
};
use domain::content::values::ContentValue;
use domain::errors::DomainError;
use domain::schema::{AttributeId, DocumentType, DocumentTypeId};
use domain::system::SystemContext;

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{COL_ID, COL_STATUS, DocumentTypeNaming, SchemaNaming};
use crate::persistence::query::common::{
    compile_delete, compile_insert, compile_select, compile_update,
};
use crate::persistence::query::drafts::{
    DraftUpsertParams, delete_draft_query, exists_draft_for_type_query, select_draft_by_id_query,
    select_drafts_by_type_query, upsert_draft_query,
};
use crate::persistence::query::headers::{
    count_draft_enabled_headers_query, count_single_lifecycle_headers_query,
    find_display_titles_by_ids_query, list_draft_enabled_headers_query,
    list_single_lifecycle_headers_query,
};
use crate::persistence::query::occ::with_occ_retry;
use crate::persistence::query::wide_table::{
    count_published_query, delete_by_id_query, exists_in_wide_table_query, find_published_query,
    select_by_id_query, select_clean_published_query, update_has_pending_draft_query,
    upsert_wide_table_query,
};
use crate::persistence::repositories::codec::{
    publication_state_to_draft_status_str, serialize_display_values_json, serialize_fields_json,
    serialize_relations_json,
};
use crate::persistence::repositories::document_snapshot::{
    SqlxDocumentSnapshotRepository, execute_insert_snapshot,
};
use crate::persistence::repositories::mappers::{
    extract_wide_published_meta, map_display_title_row, map_draft_row, map_header_row,
    map_wide_table_row,
};
use crate::persistence::repositories::relations::{
    batch_fetch_relations, load_owning_relations_batch, load_owning_relations_for_instance,
    populate_header_relational_targets, sync_owning_relations_in_tx,
};

// ---------------------------------------------------------------------------
// Public Repository Struct & Constructors
// ---------------------------------------------------------------------------

/// PostgreSQL / Aurora DSQL adapter for `DocumentInstanceRepository` and `DocumentSnapshotRepository`.
#[derive(Debug, Clone)]
pub struct SqlxDocumentInstanceRepository {
    pool: PgPool,
    context: &'static SystemContext,
    naming: &'static SchemaNaming,
}

impl SqlxDocumentInstanceRepository {
    #[must_use]
    pub const fn new(
        pool: PgPool,
        context: &'static SystemContext,
        naming: &'static SchemaNaming,
    ) -> Self {
        Self {
            pool,
            context,
            naming,
        }
    }

    #[must_use]
    pub fn snapshot_repository(&self) -> SqlxDocumentSnapshotRepository {
        SqlxDocumentSnapshotRepository::new(self.pool.clone(), self.context, self.naming)
    }
}

// ---------------------------------------------------------------------------
// Trait Implementation: DocumentSnapshotRepository
// ---------------------------------------------------------------------------

impl DocumentSnapshotRepository for SqlxDocumentInstanceRepository {
    async fn save_snapshot(&self, snapshot: &DocumentSnapshot) -> Result<(), DomainError> {
        self.snapshot_repository().save_snapshot(snapshot).await
    }

    async fn list_snapshots(
        &self,
        type_id: &DocumentTypeId,
        doc_id: DocumentInstanceId,
    ) -> Result<Vec<SnapshotHeader>, DomainError> {
        self.snapshot_repository().list_snapshots(type_id, doc_id).await
    }

    async fn get_snapshot(
        &self,
        type_id: &DocumentTypeId,
        doc_id: DocumentInstanceId,
        revision: u32,
    ) -> Result<Option<DocumentSnapshot>, DomainError> {
        self.snapshot_repository()
            .get_snapshot(type_id, doc_id, revision)
            .await
    }
}

// ---------------------------------------------------------------------------
// Trait Implementation: DocumentInstanceRepository
// ---------------------------------------------------------------------------

impl DocumentInstanceRepository for SqlxDocumentInstanceRepository {
    async fn find_by_id(
        &self,
        type_id: &DocumentTypeId,
        id: DocumentInstanceId,
        view: VersionView,
    ) -> Result<Option<DocumentInstance>, DomainError> {
        let type_naming = self.naming.require(type_id)?;
        let result = match view {
            VersionView::Published => fetch_published_by_id(&self.pool, type_naming, *id).await?,
            VersionView::Working => fetch_working_by_id(&self.pool, type_naming, *id).await?,
        };
        Ok(result)
    }

    async fn find_by_type(
        &self,
        type_id: &DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> Result<Page<DocumentInstance>, DomainError> {
        let doc_type = self
            .context
            .find_type(type_id)
            .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;

        if !doc_type.options.draft_and_publish {
            return self.find_published(type_id, pagination, filters).await;
        }

        let type_naming = self.naming.require(type_id)?;
        let page = fetch_working_page(&self.pool, type_naming, &filters, pagination).await?;
        Ok(page)
    }

    async fn find_published(
        &self,
        type_id: &DocumentTypeId,
        pagination: Pagination,
        filters: Vec<FieldFilter>,
    ) -> Result<Page<DocumentInstance>, DomainError> {
        let type_naming = self.naming.require(type_id)?;
        let total = fetch_published_count(&self.pool, type_naming, &filters).await?;
        if total == 0 {
            return Ok(Page {
                items: Vec::new(),
                total: 0,
                page: pagination.page,
                page_size: pagination.page_size,
            });
        }

        let items = fetch_published_page(&self.pool, type_naming, &filters, pagination).await?;
        Ok(Page {
            items,
            total,
            page: pagination.page,
            page_size: pagination.page_size,
        })
    }

    async fn list_admin_headers(
        &self,
        type_id: &DocumentTypeId,
        pagination: Pagination,
        status_filter: Option<DocumentStatus>,
    ) -> Result<Page<DocumentHeader>, DomainError> {
        let doc_type = self
            .context
            .find_type(type_id)
            .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
        let type_naming = self.naming.require(type_id)?;

        let total =
            fetch_admin_headers_count(&self.pool, doc_type, type_naming, status_filter).await?;
        if total == 0 {
            return Ok(Page {
                items: Vec::new(),
                total: 0,
                page: pagination.page,
                page_size: pagination.page_size,
            });
        }

        let mut items = fetch_admin_headers_page(
            &self.pool,
            doc_type,
            type_naming,
            pagination,
            status_filter,
        )
        .await?;

        populate_header_relational_targets(&self.pool, doc_type, type_naming, &mut items).await?;

        Ok(Page {
            items,
            total,
            page: pagination.page,
            page_size: pagination.page_size,
        })
    }

    async fn find_display_titles_by_ids(
        &self,
        type_id: &DocumentTypeId,
        ids: &[DocumentInstanceId],
    ) -> Result<HashMap<DocumentInstanceId, String>, DomainError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let doc_type = self
            .context
            .find_type(type_id)
            .ok_or_else(|| DomainError::DocumentTypeNotFound(type_id.clone()))?;
        let type_naming = self.naming.require(type_id)?;

        let uuids: Vec<Uuid> = ids.iter().map(|id| **id).collect();
        let stmt = find_display_titles_by_ids_query(
            type_naming,
            &uuids,
            doc_type.options.draft_and_publish,
        );
        let (sql, values) = compile_select(&stmt);
        let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_all(&self.pool)
            .await
            .map_err(PersistenceError::from)?;

        rows.iter()
            .map(map_display_title_row)
            .collect::<Result<_, _>>()
            .map_err(DomainError::from)
    }

    async fn count(
        &self,
        type_id: &DocumentTypeId,
        filters: Vec<FieldFilter>,
    ) -> Result<u64, DomainError> {
        let type_naming = self.naming.require(type_id)?;
        let total = fetch_published_count(&self.pool, type_naming, &filters).await?;
        Ok(total)
    }

    async fn fetch_relations(
        &self,
        type_id: &DocumentTypeId,
        attributes: &[AttributeId],
        parent_ids: &[DocumentInstanceId],
    ) -> Result<RelationMap, DomainError> {
        let type_naming = self.naming.require(type_id)?;
        let relation_map =
            batch_fetch_relations(&self.pool, self.naming, type_naming, attributes, parent_ids)
                .await?;
        Ok(relation_map)
    }

    async fn save_draft(
        &self,
        instance: &DocumentInstance,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
    ) -> Result<(), DomainError> {
        let type_naming = self.naming.require(&instance.document_type_id)?;
        with_occ_retry(|| async {
            save_draft_in_tx(
                &self.pool,
                type_naming,
                instance,
                display_values,
                display_title,
            )
            .await
        })
        .await
        .map_err(DomainError::from)
    }

    async fn publish(
        &self,
        instance: &DocumentInstance,
        revision: u32,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
        archived_snapshot: Option<&DocumentSnapshot>,
    ) -> Result<(), DomainError> {
        let type_naming = self.naming.require(&instance.document_type_id)?;
        with_occ_retry(|| async {
            publish_in_tx(
                &self.pool,
                type_naming,
                instance,
                revision,
                display_values,
                display_title,
                archived_snapshot,
            )
            .await
        })
        .await
        .map_err(DomainError::from)
    }

    async fn unpublish(
        &self,
        instance: &DocumentInstance,
        display_values: &IndexMap<AttributeId, String>,
        display_title: &str,
        snapshot: &DocumentSnapshot,
    ) -> Result<(), DomainError> {
        let type_naming = self.naming.require(&instance.document_type_id)?;
        with_occ_retry(|| async {
            unpublish_in_tx(
                &self.pool,
                type_naming,
                instance,
                display_values,
                display_title,
                snapshot,
            )
            .await
        })
        .await
        .map_err(DomainError::from)
    }

    async fn discard_draft(
        &self,
        type_id: &DocumentTypeId,
        id: DocumentInstanceId,
    ) -> Result<Option<DocumentInstance>, DomainError> {
        let type_naming = self.naming.require(type_id)?;
        with_occ_retry(|| async {
            discard_draft_in_tx(&self.pool, type_naming, type_id, *id).await
        })
        .await?;
        let published = fetch_published_by_id(&self.pool, type_naming, *id).await?;
        Ok(published)
    }

    async fn delete(
        &self,
        type_id: &DocumentTypeId,
        id: DocumentInstanceId,
    ) -> Result<(), DomainError> {
        let type_naming = self.naming.require(type_id)?;
        with_occ_retry(|| async { delete_in_tx(&self.pool, type_naming, type_id, *id).await })
            .await
            .map_err(DomainError::from)
    }

    async fn exists_for_type(&self, type_id: &DocumentTypeId) -> Result<bool, DomainError> {
        let type_naming = self.naming.require(type_id)?;
        let (sql_wide, val_wide) = compile_select(&exists_in_wide_table_query(type_naming));
        let has_wide = sqlx::query_with(AssertSqlSafe(sql_wide.as_str()), val_wide)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?
            .is_some();
        if has_wide {
            return Ok(true);
        }

        let (sql_draft, val_draft) = compile_select(&exists_draft_for_type_query(type_id));
        let has_draft = sqlx::query_with(AssertSqlSafe(sql_draft.as_str()), val_draft)
            .fetch_optional(&self.pool)
            .await
            .map_err(PersistenceError::from)?
            .is_some();
        Ok(has_draft)
    }
}

// ---------------------------------------------------------------------------
// Private Helpers: Single & Page Queries
// ---------------------------------------------------------------------------

async fn fetch_published_by_id(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    id: Uuid,
) -> Result<Option<DocumentInstance>, PersistenceError> {
    let (sql, values) = compile_select(&select_by_id_query(type_naming, id));
    let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_optional(pool)
        .await
        .map_err(PersistenceError::from)?;

    let Some(row) = maybe_row else {
        return Ok(None);
    };

    let relations = load_owning_relations_for_instance(pool, type_naming, id).await?;
    let instance = map_wide_table_row(&row, type_naming, relations)?;
    Ok(Some(instance))
}

async fn fetch_working_by_id(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    id: Uuid,
) -> Result<Option<DocumentInstance>, PersistenceError> {
    let (sql, values) =
        compile_select(&select_draft_by_id_query(&type_naming.document_type_id, id));
    let maybe_draft = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_optional(pool)
        .await
        .map_err(PersistenceError::from)?;

    if let Some(row) = maybe_draft {
        let status_str: String = row.try_get(COL_STATUS)?;
        let live_meta = if status_str == "modified" {
            fetch_wide_published_meta(pool, type_naming, id).await?
        } else {
            None
        };
        let instance = map_draft_row(&row, type_naming, live_meta)?;
        return Ok(Some(instance));
    }

    fetch_published_by_id(pool, type_naming, id).await
}

async fn fetch_wide_published_meta(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    id: Uuid,
) -> Result<Option<(u32, DateTime<Utc>, ActorId)>, PersistenceError> {
    let (sql, values) = compile_select(&select_by_id_query(type_naming, id));
    let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_optional(pool)
        .await
        .map_err(PersistenceError::from)?;
    maybe_row
        .map(|r| extract_wide_published_meta(&r))
        .transpose()
}

async fn fetch_published_count(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    filters: &[FieldFilter],
) -> Result<u64, PersistenceError> {
    let stmt = count_published_query(type_naming, filters)?;
    let (sql, values) = compile_select(&stmt);
    let row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_one(pool)
        .await
        .map_err(PersistenceError::from)?;
    let count: i64 = row.try_get(0)?;
    Ok(u64::try_from(count).unwrap_or(0))
}

async fn fetch_published_page(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    filters: &[FieldFilter],
    pagination: Pagination,
) -> Result<Vec<DocumentInstance>, PersistenceError> {
    let stmt = find_published_query(type_naming, filters, pagination)?;
    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;
    assemble_published_instances(pool, type_naming, &rows).await
}

async fn assemble_published_instances(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    rows: &[PgRow],
) -> Result<Vec<DocumentInstance>, PersistenceError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<Uuid> = rows
        .iter()
        .map(|r| r.try_get(COL_ID))
        .collect::<Result<_, _>>()?;
    let mut relations_by_id = load_owning_relations_batch(pool, type_naming, &ids).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let id: Uuid = row.try_get(COL_ID)?;
        let relations = relations_by_id.remove(&id).unwrap_or_default();
        items.push(map_wide_table_row(row, type_naming, relations)?);
    }
    Ok(items)
}

async fn fetch_working_page(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    filters: &[FieldFilter],
    pagination: Pagination,
) -> Result<Page<DocumentInstance>, PersistenceError> {
    let mut combined = fetch_draft_instances(pool, type_naming).await?;
    let clean_published = fetch_clean_published_instances(pool, type_naming).await?;
    combined.extend(clean_published);

    if !filters.is_empty() {
        combined.retain(|inst| matches_filters(inst, filters));
    }

    combined.sort_by_key(|inst| (inst.audit.created_at, *inst.id.as_ref()));

    let total = u64::try_from(combined.len()).unwrap_or(0);
    let start =
        (u64::from(pagination.page.saturating_sub(1)) * u64::from(pagination.page_size)) as usize;
    let items = combined
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

fn matches_filters(instance: &DocumentInstance, filters: &[FieldFilter]) -> bool {
    filters.iter().all(|f| {
        matches!(
            instance.get_field(&f.attribute_id),
            Some(ContentValue::Scalar(v)) if v == &f.value
        )
    })
}

async fn fetch_clean_published_instances(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
) -> Result<Vec<DocumentInstance>, PersistenceError> {
    let stmt = select_clean_published_query(type_naming);
    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;
    assemble_published_instances(pool, type_naming, &rows).await
}

async fn fetch_draft_instances(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
) -> Result<Vec<DocumentInstance>, PersistenceError> {
    let stmt = select_drafts_by_type_query(&type_naming.document_type_id);
    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;

    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        let status_str: String = row.try_get(COL_STATUS)?;
        let live_meta = if status_str == "modified" {
            let id: Uuid = row.try_get(COL_ID)?;
            fetch_wide_published_meta(pool, type_naming, id).await?
        } else {
            None
        };
        items.push(map_draft_row(row, type_naming, live_meta)?);
    }
    Ok(items)
}

async fn fetch_admin_headers_count(
    pool: &PgPool,
    doc_type: &DocumentType,
    type_naming: &DocumentTypeNaming,
    status_filter: Option<DocumentStatus>,
) -> Result<u64, PersistenceError> {
    let stmt = if doc_type.options.draft_and_publish {
        count_draft_enabled_headers_query(type_naming, status_filter)
    } else {
        count_single_lifecycle_headers_query(type_naming, status_filter)
    };
    let (sql, values) = compile_select(&stmt);
    let row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_one(pool)
        .await
        .map_err(PersistenceError::from)?;
    let count: i64 = row.try_get(0)?;
    Ok(u64::try_from(count).unwrap_or(0))
}

async fn fetch_admin_headers_page(
    pool: &PgPool,
    doc_type: &DocumentType,
    type_naming: &DocumentTypeNaming,
    pagination: Pagination,
    status_filter: Option<DocumentStatus>,
) -> Result<Vec<DocumentHeader>, PersistenceError> {
    let stmt = if doc_type.options.draft_and_publish {
        list_draft_enabled_headers_query(type_naming, pagination, status_filter)
    } else {
        list_single_lifecycle_headers_query(type_naming, pagination, status_filter)
    };
    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;
    rows.iter()
        .map(|r| map_header_row(r, &type_naming.document_type_id))
        .collect()
}

// ---------------------------------------------------------------------------
// Private Helpers: Transactions & Mutations
// ---------------------------------------------------------------------------

async fn save_draft_in_tx(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    instance: &DocumentInstance,
    display_values: &IndexMap<AttributeId, String>,
    display_title: &str,
) -> Result<(), PersistenceError> {
    let mut tx = pool.begin().await?;
    execute_upsert_draft(&mut tx, instance, display_values, display_title).await?;

    if matches!(
        instance.content.publication_state,
        PublicationState::Modified { .. }
    ) {
        let stmt = update_has_pending_draft_query(type_naming, *instance.id, true);
        let (sql, values) = compile_update(&stmt);
        sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .execute(&mut *tx)
            .await
            .map_err(PersistenceError::from)?;
    }

    tx.commit().await?;
    Ok(())
}

async fn execute_upsert_draft(
    conn: &mut PgConnection,
    instance: &DocumentInstance,
    display_values: &IndexMap<AttributeId, String>,
    display_title: &str,
) -> Result<(), PersistenceError> {
    let (status, last_published_revision) =
        publication_state_to_draft_status_str(&instance.content.publication_state)?;
    let display_values_json = serialize_display_values_json(display_values)?;
    let content_json = serialize_fields_json(instance.content.fields())?;
    let relations_json = serialize_relations_json(&instance.relations)?;
    let version = i32::try_from(instance.audit.version).unwrap_or(1);

    let params = DraftUpsertParams {
        id: *instance.id,
        document_type_id: &instance.document_type_id,
        display_values_json,
        display_title,
        status,
        content_json,
        relations_json,
        schema_version: 1,
        version,
        last_published_revision,
        created_at: instance.audit.created_at,
        created_by_urn: instance.audit.created_by.to_urn(),
        updated_at: instance.audit.updated_at,
        updated_by_urn: instance.audit.updated_by.to_urn(),
    };

    let stmt = upsert_draft_query(&params);
    let (sql, values) = compile_insert(&stmt);
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(conn)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

async fn publish_in_tx(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    instance: &DocumentInstance,
    revision: u32,
    display_values: &IndexMap<AttributeId, String>,
    display_title: &str,
    archived_snapshot: Option<&DocumentSnapshot>,
) -> Result<(), PersistenceError> {
    let mut tx = pool.begin().await?;

    if let Some(snapshot) = archived_snapshot {
        execute_insert_snapshot(&mut tx, snapshot).await?;
    }

    let display_values_json = serialize_display_values_json(display_values)?;
    let upsert_stmt = upsert_wide_table_query(
        type_naming,
        instance,
        revision,
        display_values_json,
        display_title,
    )?;
    let (up_sql, up_values) = compile_insert(&upsert_stmt);
    sqlx::query_with(AssertSqlSafe(up_sql.as_str()), up_values)
        .execute(&mut *tx)
        .await
        .map_err(PersistenceError::from)?;

    sync_owning_relations_in_tx(&mut tx, type_naming, instance).await?;

    let del_stmt = delete_draft_query(&instance.document_type_id, *instance.id);
    let (del_sql, del_values) = compile_delete(&del_stmt);
    sqlx::query_with(AssertSqlSafe(del_sql.as_str()), del_values)
        .execute(&mut *tx)
        .await
        .map_err(PersistenceError::from)?;

    tx.commit().await?;
    Ok(())
}

async fn unpublish_in_tx(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    instance: &DocumentInstance,
    display_values: &IndexMap<AttributeId, String>,
    display_title: &str,
    snapshot: &DocumentSnapshot,
) -> Result<(), PersistenceError> {
    let mut tx = pool.begin().await?;

    execute_insert_snapshot(&mut tx, snapshot).await?;

    let del_stmt = delete_by_id_query(type_naming, *instance.id);
    let (del_sql, del_values) = compile_delete(&del_stmt);
    sqlx::query_with(AssertSqlSafe(del_sql.as_str()), del_values)
        .execute(&mut *tx)
        .await
        .map_err(PersistenceError::from)?;

    execute_upsert_draft(&mut tx, instance, display_values, display_title).await?;

    tx.commit().await?;
    Ok(())
}

async fn discard_draft_in_tx(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    type_id: &DocumentTypeId,
    id: Uuid,
) -> Result<(), PersistenceError> {
    let mut tx = pool.begin().await?;

    let del_stmt = delete_draft_query(type_id, id);
    let (del_sql, del_values) = compile_delete(&del_stmt);
    sqlx::query_with(AssertSqlSafe(del_sql.as_str()), del_values)
        .execute(&mut *tx)
        .await
        .map_err(PersistenceError::from)?;

    let update_stmt = update_has_pending_draft_query(type_naming, id, false);
    let (up_sql, up_values) = compile_update(&update_stmt);
    sqlx::query_with(AssertSqlSafe(up_sql.as_str()), up_values)
        .execute(&mut *tx)
        .await
        .map_err(PersistenceError::from)?;

    tx.commit().await?;
    Ok(())
}

async fn delete_in_tx(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    type_id: &DocumentTypeId,
    id: Uuid,
) -> Result<(), PersistenceError> {
    let mut tx = pool.begin().await?;

    let del_draft = delete_draft_query(type_id, id);
    let (sql_draft, val_draft) = compile_delete(&del_draft);
    sqlx::query_with(AssertSqlSafe(sql_draft.as_str()), val_draft)
        .execute(&mut *tx)
        .await
        .map_err(PersistenceError::from)?;

    let del_wide = delete_by_id_query(type_naming, id);
    let (sql_wide, val_wide) = compile_delete(&del_wide);
    sqlx::query_with(AssertSqlSafe(sql_wide.as_str()), val_wide)
        .execute(&mut *tx)
        .await
        .map_err(PersistenceError::from)?;

    tx.commit().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use domain::content::instance::{AuditTrail, DocumentContent};
    use domain::content::values::{DomainValue, PrimitiveValue};
    use domain::schema::SchemaRegistry;
    use domain::system::{LocaleId, SystemConfig, SystemConfigId};

    fn fixture_context_and_naming() -> (&'static SystemContext, &'static SchemaNaming) {
        let registry = SchemaRegistry::new(vec![]);
        let naming = SchemaNaming::from_registry(&registry);
        let locale = LocaleId::try_new("en").unwrap();
        let config = SystemConfig::new(
            SystemConfigId::new(Uuid::now_v7()),
            vec![locale.clone()],
            locale,
        )
        .unwrap();
        let context = Box::leak(Box::new(SystemContext::new(registry, config)));
        (context, naming)
    }

    #[tokio::test]
    async fn test_sqlx_document_instance_repository_new_and_snapshot_repo() {
        let (context, naming) = fixture_context_and_naming();
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test")
            .expect("lazy pool creation must succeed");

        let repo = SqlxDocumentInstanceRepository::new(pool, context, naming);
        let snapshot_repo = repo.snapshot_repository();
        assert_eq!(
            snapshot_repo.context().config.default_locale,
            context.config.default_locale
        );
    }

    #[test]
    fn test_matches_filters_evaluation() {
        let attr = AttributeId::try_new("title").unwrap();
        let mut fields = HashMap::new();
        fields.insert(
            attr.clone(),
            ContentValue::Scalar(DomainValue::Primitive(PrimitiveValue::Text(
                "Hello".to_string(),
            ))),
        );
        let instance = DocumentInstance::reconstitute(
            DocumentInstanceId::new(Uuid::now_v7()),
            DocumentTypeId::try_new("article").unwrap(),
            DocumentContent::new(fields, PublicationState::Draft),
            HashMap::new(),
            AuditTrail {
                created_at: Utc::now(),
                created_by: ActorId::System,
                updated_at: Utc::now(),
                updated_by: ActorId::System,
                version: 1,
            },
        );

        let filter_match = FieldFilter {
            attribute_id: attr.clone(),
            value: DomainValue::Primitive(PrimitiveValue::Text("Hello".to_string())),
        };
        assert!(matches_filters(&instance, &[filter_match]));

        let filter_mismatch = FieldFilter {
            attribute_id: attr,
            value: DomainValue::Primitive(PrimitiveValue::Text("World".to_string())),
        };
        assert!(!matches_filters(&instance, &[filter_mismatch]));
    }
}
