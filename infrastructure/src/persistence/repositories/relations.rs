//! Relational junction table synchronization and two-phase batch relation enrichment.

use std::collections::{HashMap, HashSet};

use sea_query::{Asterisk, Expr, ExprTrait, Query};
use sqlx::{AssertSqlSafe, PgPool, Row, Transaction};
use uuid::Uuid;
use domain::content::header::{DocumentHeader, DocumentStatus};
use domain::content::instance::{DocumentInstance, DocumentInstanceId, ResolvedRelation};
use domain::content::ports::RelationMap;
use domain::schema::{AttributeId, DocumentType};

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{
    COL_ID, COL_RELATIONS, COL_SOURCE_ID, COL_TARGET_ID, DocumentTypeNaming, RelationNaming,
    SchemaNaming,
};
use crate::persistence::query::common::{
    compile_delete, compile_insert, compile_select, COL_DOCUMENT_TYPE_ID_IDEN, COL_ID_IDEN,
    COL_RELATIONS_IDEN, TABLE_DOCUMENT_DRAFTS_IDEN,
};
use crate::persistence::query::junction::{
    delete_links_by_source_query, insert_links_batch_query, select_links_by_sources_query,
    select_links_by_targets_query, select_targets_by_source_query,
};
use crate::persistence::repositories::mappers::{map_draft_row, map_wide_table_row};

// ---------------------------------------------------------------------------
// Public Functions
// ---------------------------------------------------------------------------

/// Synchronizes all owning relations for a document instance within an existing transaction.
pub async fn sync_owning_relations_in_tx(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    type_naming: &DocumentTypeNaming,
    instance: &DocumentInstance,
) -> Result<(), PersistenceError> {
    for rel_naming in type_naming.relations.values() {
        if !rel_naming.relation_type.is_owning() {
            continue;
        }

        execute_delete_links(tx, rel_naming, *instance.id).await?;

        if let Some(resolved_list) = instance.relations.get(&rel_naming.attribute_id) {
            execute_insert_links(tx, rel_naming, *instance.id, resolved_list).await?;
        }
    }
    Ok(())
}

/// Loads all owning relational targets for a single instance from their junction tables.
pub async fn load_owning_relations_for_instance(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    instance_id: Uuid,
) -> Result<HashMap<AttributeId, Vec<ResolvedRelation>>, PersistenceError> {
    let mut relations = HashMap::new();

    for rel_naming in type_naming.relations.values() {
        if !rel_naming.relation_type.is_owning() {
            continue;
        }

        let list = query_targets_for_source(pool, rel_naming, instance_id).await?;
        if !list.is_empty() {
            relations.insert(rel_naming.attribute_id.clone(), list);
        }
    }

    Ok(relations)
}

/// Batch loads all owning relational targets for multiple source instances.
pub async fn load_owning_relations_batch(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    source_ids: &[Uuid],
) -> Result<HashMap<Uuid, HashMap<AttributeId, Vec<ResolvedRelation>>>, PersistenceError> {
    if source_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut result: HashMap<Uuid, HashMap<AttributeId, Vec<ResolvedRelation>>> = HashMap::new();

    for rel_naming in type_naming.relations.values() {
        if !rel_naming.relation_type.is_owning() {
            continue;
        }

        let stmt = select_links_by_sources_query(rel_naming.junction_table_iden, source_ids);
        let (sql, values) = compile_select(&stmt);
        let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .fetch_all(pool)
            .await
            .map_err(PersistenceError::from)?;

        for row in rows {
            let source: Uuid = row.try_get(COL_SOURCE_ID)?;
            let target: Uuid = row.try_get(COL_TARGET_ID)?;
            let resolved = ResolvedRelation {
                attribute_id: rel_naming.attribute_id.clone(),
                target_instance_id: DocumentInstanceId::new(target),
            };
            result
                .entry(source)
                .or_default()
                .entry(rel_naming.attribute_id.clone())
                .or_default()
                .push(resolved);
        }
    }

    Ok(result)
}

/// Performs two-phase batch relation loading for `DocumentInstanceRepository::fetch_relations`.
pub async fn batch_fetch_relations(
    pool: &PgPool,
    naming: &'static SchemaNaming,
    type_naming: &DocumentTypeNaming,
    attributes: &[AttributeId],
    parent_ids: &[DocumentInstanceId],
) -> Result<RelationMap, PersistenceError> {
    if attributes.is_empty() || parent_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let parent_uuids: Vec<Uuid> = parent_ids.iter().map(|id| **id).collect();
    let mut relation_map: RelationMap = HashMap::with_capacity(attributes.len());

    for attr in attributes {
        let Some(rel_naming) = type_naming.relation(attr) else {
            continue;
        };
        let target_type_naming = naming.require(&rel_naming.target_type_id)?;

        let pairs = fetch_junction_pairs(pool, rel_naming, &parent_uuids).await?;
        if pairs.is_empty() {
            continue;
        }

        let target_instances = fetch_target_instances(pool, target_type_naming, &pairs).await?;
        let parent_map = assemble_parent_relation_map(parent_ids, &pairs, &target_instances);
        relation_map.insert(attr.clone(), parent_map);
    }

    Ok(relation_map)
}

/// Batch populates `header.relational_display_target` for headers with relational display fields.
pub async fn populate_header_relational_targets(
    pool: &PgPool,
    doc_type: &DocumentType,
    type_naming: &DocumentTypeNaming,
    headers: &mut [DocumentHeader],
) -> Result<(), PersistenceError> {
    let Some(rel_def) = doc_type.relational_display_field() else {
        return Ok(());
    };
    if headers.is_empty() {
        return Ok(());
    }
    let Some(rel_naming) = type_naming.relation(&rel_def.id) else {
        return Ok(());
    };

    let (draft_ids, pub_ids) = partition_header_ids(headers);
    let draft_targets = fetch_draft_relational_targets(pool, type_naming, &draft_ids, &rel_def.id).await?;
    let pub_targets = fetch_published_relational_targets(pool, rel_naming, &pub_ids).await?;

    for header in headers {
        if let Some(&target_id) = draft_targets.get(&header.id) {
            header.relational_display_target = Some((rel_def.id.clone(), target_id));
        } else if let Some(&target_id) = pub_targets.get(&header.id) {
            header.relational_display_target = Some((rel_def.id.clone(), target_id));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Private Helpers: Sync & Queries
// ---------------------------------------------------------------------------

async fn execute_delete_links(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    rel_naming: &RelationNaming,
    source_id: Uuid,
) -> Result<(), PersistenceError> {
    let del_stmt = delete_links_by_source_query(rel_naming.junction_table_iden, source_id);
    let (sql, values) = compile_delete(&del_stmt);
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(&mut **tx)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

async fn execute_insert_links(
    tx: &mut Transaction<'_, sqlx::Postgres>,
    rel_naming: &RelationNaming,
    source_id: Uuid,
    resolved_list: &[ResolvedRelation],
) -> Result<(), PersistenceError> {
    if resolved_list.is_empty() {
        return Ok(());
    }

    let target_ids: Vec<DocumentInstanceId> =
        resolved_list.iter().map(|r| r.target_instance_id).collect();

    if let Some(ins_stmt) =
        insert_links_batch_query(rel_naming.junction_table_iden, source_id, &target_ids)
    {
        let (sql, values) = compile_insert(&ins_stmt);
        sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
            .execute(&mut **tx)
            .await
            .map_err(PersistenceError::from)?;
    }
    Ok(())
}

async fn query_targets_for_source(
    pool: &PgPool,
    rel_naming: &RelationNaming,
    instance_id: Uuid,
) -> Result<Vec<ResolvedRelation>, PersistenceError> {
    let stmt = select_targets_by_source_query(rel_naming.junction_table_iden, instance_id);
    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;

    let mut list = Vec::with_capacity(rows.len());
    for row in rows {
        let target_uuid: Uuid = row.try_get(COL_TARGET_ID)?;
        list.push(ResolvedRelation {
            attribute_id: rel_naming.attribute_id.clone(),
            target_instance_id: DocumentInstanceId::new(target_uuid),
        });
    }
    Ok(list)
}

// ---------------------------------------------------------------------------
// Private Helpers: Batch Relations
// ---------------------------------------------------------------------------

async fn fetch_junction_pairs(
    pool: &PgPool,
    rel_naming: &RelationNaming,
    parent_uuids: &[Uuid],
) -> Result<Vec<(DocumentInstanceId, DocumentInstanceId)>, PersistenceError> {
    let stmt = if rel_naming.relation_type.is_owning() {
        select_links_by_sources_query(rel_naming.junction_table_iden, parent_uuids)
    } else {
        select_links_by_targets_query(rel_naming.junction_table_iden, parent_uuids)
    };

    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;

    let mut pairs = Vec::with_capacity(rows.len());
    for row in rows {
        let source_id: Uuid = row.try_get(COL_SOURCE_ID)?;
        let target_id: Uuid = row.try_get(COL_TARGET_ID)?;
        let (parent, related) = if rel_naming.relation_type.is_owning() {
            (
                DocumentInstanceId::new(source_id),
                DocumentInstanceId::new(target_id),
            )
        } else {
            (
                DocumentInstanceId::new(target_id),
                DocumentInstanceId::new(source_id),
            )
        };
        pairs.push((parent, related));
    }
    Ok(pairs)
}

async fn fetch_target_instances(
    pool: &PgPool,
    target_type_naming: &DocumentTypeNaming,
    pairs: &[(DocumentInstanceId, DocumentInstanceId)],
) -> Result<HashMap<DocumentInstanceId, DocumentInstance>, PersistenceError> {
    let unique_target_uuids: Vec<Uuid> = pairs
        .iter()
        .map(|(_, target)| **target)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let mut instances =
        fetch_wide_target_instances(pool, target_type_naming, &unique_target_uuids).await?;

    let missing: Vec<Uuid> = unique_target_uuids
        .into_iter()
        .filter(|id| !instances.contains_key(&DocumentInstanceId::new(*id)))
        .collect();

    if !missing.is_empty() {
        let draft_instances =
            fetch_draft_target_instances(pool, target_type_naming, &missing).await?;
        instances.extend(draft_instances);
    }

    Ok(instances)
}

async fn fetch_wide_target_instances(
    pool: &PgPool,
    target_type_naming: &DocumentTypeNaming,
    uuids: &[Uuid],
) -> Result<HashMap<DocumentInstanceId, DocumentInstance>, PersistenceError> {
    if uuids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut stmt = Query::select();
    stmt.from(target_type_naming.table_iden)
        .column(Asterisk)
        .and_where(Expr::col(COL_ID_IDEN).is_in(uuids.to_vec()));

    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;

    let found_uuids: Vec<Uuid> = rows.iter().filter_map(|r| r.try_get(COL_ID).ok()).collect();
    let mut owning_relations =
        load_owning_relations_batch(pool, target_type_naming, &found_uuids).await?;

    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let row_id: Uuid = row.try_get(COL_ID)?;
        let rels = owning_relations.remove(&row_id).unwrap_or_default();
        let instance = map_wide_table_row(&row, target_type_naming, rels)?;
        map.insert(instance.id, instance);
    }
    Ok(map)
}

async fn fetch_draft_target_instances(
    pool: &PgPool,
    target_type_naming: &DocumentTypeNaming,
    missing: &[Uuid],
) -> Result<HashMap<DocumentInstanceId, DocumentInstance>, PersistenceError> {
    if missing.is_empty() {
        return Ok(HashMap::new());
    }

    let mut stmt = Query::select();
    stmt.from(TABLE_DOCUMENT_DRAFTS_IDEN)
        .column(Asterisk)
        .and_where(
            Expr::col(COL_DOCUMENT_TYPE_ID_IDEN)
                .eq(target_type_naming.document_type_id.as_ref()),
        )
        .and_where(Expr::col(COL_ID_IDEN).is_in(missing.to_vec()));

    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;

    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let instance = map_draft_row(&row, target_type_naming, None)?;
        map.insert(instance.id, instance);
    }
    Ok(map)
}

fn assemble_parent_relation_map(
    parent_ids: &[DocumentInstanceId],
    pairs: &[(DocumentInstanceId, DocumentInstanceId)],
    target_instances: &HashMap<DocumentInstanceId, DocumentInstance>,
) -> HashMap<DocumentInstanceId, Vec<DocumentInstance>> {
    let mut map: HashMap<DocumentInstanceId, Vec<DocumentInstance>> =
        HashMap::with_capacity(parent_ids.len());
    for &(parent, target) in pairs {
        if let Some(instance) = target_instances.get(&target) {
            map.entry(parent).or_default().push(instance.clone());
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Private Helpers: Header Relational Targets
// ---------------------------------------------------------------------------

fn partition_header_ids(
    headers: &[DocumentHeader],
) -> (Vec<Uuid>, Vec<Uuid>) {
    let mut draft_ids = Vec::new();
    let mut pub_ids = Vec::new();
    for header in headers {
        if header.status == DocumentStatus::Published {
            pub_ids.push(*header.id);
        } else {
            draft_ids.push(*header.id);
        }
    }
    (draft_ids, pub_ids)
}

async fn fetch_draft_relational_targets(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    draft_ids: &[Uuid],
    rel_attr: &AttributeId,
) -> Result<HashMap<DocumentInstanceId, DocumentInstanceId>, PersistenceError> {
    if draft_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut stmt = Query::select();
    stmt.from(TABLE_DOCUMENT_DRAFTS_IDEN)
        .columns([COL_ID_IDEN, COL_RELATIONS_IDEN])
        .and_where(Expr::col(COL_DOCUMENT_TYPE_ID_IDEN).eq(type_naming.document_type_id.as_ref()))
        .and_where(Expr::col(COL_ID_IDEN).is_in(draft_ids.to_vec()));

    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;

    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let id = DocumentInstanceId::new(row.try_get(COL_ID)?);
        let rel_val: serde_json::Value = row.try_get(COL_RELATIONS)?;
        if let Some(target_id) = extract_first_relation_target(&rel_val, rel_attr) {
            map.insert(id, target_id);
        }
    }
    Ok(map)
}

fn extract_first_relation_target(
    rel_val: &serde_json::Value,
    rel_attr: &AttributeId,
) -> Option<DocumentInstanceId> {
    let uuid_str = rel_val
        .get(rel_attr.as_ref())
        .and_then(serde_json::Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(serde_json::Value::as_str)?;
    Uuid::parse_str(uuid_str).ok().map(DocumentInstanceId::new)
}

async fn fetch_published_relational_targets(
    pool: &PgPool,
    rel_naming: &RelationNaming,
    pub_ids: &[Uuid],
) -> Result<HashMap<DocumentInstanceId, DocumentInstanceId>, PersistenceError> {
    if pub_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let stmt = if rel_naming.relation_type.is_owning() {
        select_links_by_sources_query(rel_naming.junction_table_iden, pub_ids)
    } else {
        select_links_by_targets_query(rel_naming.junction_table_iden, pub_ids)
    };

    let (sql, values) = compile_select(&stmt);
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;

    let mut map = HashMap::with_capacity(rows.len());
    for row in rows {
        let source_id: Uuid = row.try_get(COL_SOURCE_ID)?;
        let target_id: Uuid = row.try_get(COL_TARGET_ID)?;
        let (parent, related) = if rel_naming.relation_type.is_owning() {
            (
                DocumentInstanceId::new(source_id),
                DocumentInstanceId::new(target_id),
            )
        } else {
            (
                DocumentInstanceId::new(target_id),
                DocumentInstanceId::new(source_id),
            )
        };
        map.entry(parent).or_insert(related);
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::auth::user::ActorId;
    use domain::schema::DocumentTypeId;
    use indexmap::IndexMap;

    fn make_test_header(id: DocumentInstanceId, status: DocumentStatus) -> DocumentHeader {
        DocumentHeader {
            id,
            document_type_id: DocumentTypeId::try_new("article").unwrap(),
            display_values: IndexMap::new(),
            display_title: "Title".into(),
            relational_display_target: None,
            status,
            slug: None,
            revision: None,
            draft_version: None,
            created_at: Utc::now(),
            created_by: ActorId::System,
            updated_at: Utc::now(),
            updated_by: ActorId::System,
            published_at: None,
            published_by: None,
        }
    }

    #[test]
    fn test_partition_header_ids() {
        let id1 = DocumentInstanceId::new(Uuid::now_v7());
        let id2 = DocumentInstanceId::new(Uuid::now_v7());
        let id3 = DocumentInstanceId::new(Uuid::now_v7());

        let headers = vec![
            make_test_header(id1, DocumentStatus::Published),
            make_test_header(id2, DocumentStatus::Draft),
            make_test_header(id3, DocumentStatus::Modified),
        ];

        let (draft_ids, pub_ids) = partition_header_ids(&headers);
        assert_eq!(pub_ids, vec![*id1]);
        assert_eq!(draft_ids, vec![*id2, *id3]);
    }

    #[test]
    fn test_assemble_parent_relation_map() {
        let parent1 = DocumentInstanceId::new(Uuid::now_v7());
        let target1 = DocumentInstanceId::new(Uuid::now_v7());
        let mut target_instances = HashMap::new();
        let target_doc = DocumentInstance::new(
            DocumentTypeId::try_new("tag").unwrap(),
            ActorId::System,
            Utc::now(),
        );
        let actual_target_id = target_doc.id;
        target_instances.insert(actual_target_id, target_doc.clone());

        let pairs = vec![(parent1, actual_target_id), (parent1, target1)];
        let assembled = assemble_parent_relation_map(&[parent1], &pairs, &target_instances);

        assert_eq!(assembled.len(), 1);
        let list = assembled.get(&parent1).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, actual_target_id);
    }

    #[test]
    fn test_extract_first_relation_target() {
        let attr = AttributeId::try_new("category").unwrap();
        let target_uuid = Uuid::now_v7();

        // 1. Valid relation array with one or more UUIDs
        let valid_json = serde_json::json!({
            "category": [target_uuid.to_string(), Uuid::now_v7().to_string()]
        });
        assert_eq!(
            extract_first_relation_target(&valid_json, &attr),
            Some(DocumentInstanceId::new(target_uuid))
        );

        // 2. Empty array
        let empty_json = serde_json::json!({
            "category": []
        });
        assert_eq!(extract_first_relation_target(&empty_json, &attr), None);

        // 3. Missing attribute
        let missing_json = serde_json::json!({
            "other": [target_uuid.to_string()]
        });
        assert_eq!(extract_first_relation_target(&missing_json, &attr), None);

        // 4. Invalid UUID string
        let invalid_uuid_json = serde_json::json!({
            "category": ["not-a-valid-uuid"]
        });
        assert_eq!(extract_first_relation_target(&invalid_uuid_json, &attr), None);
    }
}
