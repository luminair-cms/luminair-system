//! SQLx implementation of `DocumentSnapshotRepository`.

use sqlx::{AssertSqlSafe, PgConnection, PgPool};
use uuid::Uuid;

use domain::content::header::{DocumentSnapshot, SnapshotHeader};
use domain::content::instance::DocumentInstanceId;
use domain::content::ports::DocumentSnapshotRepository;
use domain::errors::DomainError;
use domain::schema::DocumentTypeId;
use domain::system::SystemContext;

use crate::persistence::error::PersistenceError;
use crate::persistence::naming::{DocumentTypeNaming, SchemaNaming};
use crate::persistence::query::common::{compile_insert, compile_select};
use crate::persistence::query::occ::with_occ_retry;
use crate::persistence::query::snapshots::{
    get_snapshot_by_revision_query, insert_snapshot_query, list_snapshot_headers_query,
};
use crate::persistence::repositories::codec::{
    serialize_fields_json, serialize_relations_json, snapshot_reason_to_db_str,
};
use crate::persistence::repositories::mappers::{map_snapshot_header_row, map_snapshot_row};

// ---------------------------------------------------------------------------
// Public Repository Struct & Constructors
// ---------------------------------------------------------------------------

/// PostgreSQL / Aurora DSQL adapter for `DocumentSnapshotRepository`.
#[derive(Debug, Clone)]
pub struct SqlxDocumentSnapshotRepository {
    pool: PgPool,
    context: &'static SystemContext,
    naming: &'static SchemaNaming,
}

impl SqlxDocumentSnapshotRepository {
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
    pub const fn context(&self) -> &'static SystemContext {
        self.context
    }
}

// ---------------------------------------------------------------------------
// Trait Implementation: DocumentSnapshotRepository
// ---------------------------------------------------------------------------

impl DocumentSnapshotRepository for SqlxDocumentSnapshotRepository {
    async fn save_snapshot(&self, snapshot: &DocumentSnapshot) -> Result<(), DomainError> {
        self.naming.require(&snapshot.document_type_id)?;
        with_occ_retry(|| async { save_snapshot_in_tx(&self.pool, snapshot).await })
            .await
            .map_err(DomainError::from)
    }

    async fn list_snapshots(
        &self,
        type_id: &DocumentTypeId,
        doc_id: DocumentInstanceId,
    ) -> Result<Vec<SnapshotHeader>, DomainError> {
        self.naming.require(type_id)?;
        let headers = fetch_snapshot_headers(&self.pool, type_id, *doc_id).await?;
        Ok(headers)
    }

    async fn get_snapshot(
        &self,
        type_id: &DocumentTypeId,
        doc_id: DocumentInstanceId,
        revision: u32,
    ) -> Result<Option<DocumentSnapshot>, DomainError> {
        let type_naming = self.naming.require(type_id)?;
        let maybe_snapshot =
            fetch_snapshot_by_revision(&self.pool, type_naming, *doc_id, revision).await?;
        Ok(maybe_snapshot)
    }
}

// ---------------------------------------------------------------------------
// Public Crate Functions
// ---------------------------------------------------------------------------

/// Inserts a document snapshot row into `document_snapshots` within an existing connection/transaction.
pub(crate) async fn execute_insert_snapshot(
    conn: &mut PgConnection,
    snapshot: &DocumentSnapshot,
) -> Result<(), PersistenceError> {
    let content_json = serialize_fields_json(&snapshot.fields)?;
    let relations_json = serialize_relations_json(&snapshot.relations)?;
    let reason_str = snapshot_reason_to_db_str(snapshot.reason);
    let stmt = insert_snapshot_query(snapshot, content_json, relations_json, reason_str);
    let (sql, values) = compile_insert(&stmt);
    sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .execute(conn)
        .await
        .map_err(PersistenceError::from)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Private Helpers
// ---------------------------------------------------------------------------

async fn save_snapshot_in_tx(
    pool: &PgPool,
    snapshot: &DocumentSnapshot,
) -> Result<(), PersistenceError> {
    let mut tx = pool.begin().await?;
    execute_insert_snapshot(&mut tx, snapshot).await?;
    tx.commit().await?;
    Ok(())
}

async fn fetch_snapshot_headers(
    pool: &PgPool,
    type_id: &DocumentTypeId,
    doc_id: Uuid,
) -> Result<Vec<SnapshotHeader>, PersistenceError> {
    let (sql, values) = compile_select(&list_snapshot_headers_query(type_id, doc_id));
    let rows = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_all(pool)
        .await
        .map_err(PersistenceError::from)?;
    rows.iter().map(map_snapshot_header_row).collect()
}

async fn fetch_snapshot_by_revision(
    pool: &PgPool,
    type_naming: &DocumentTypeNaming,
    doc_id: Uuid,
    revision: u32,
) -> Result<Option<DocumentSnapshot>, PersistenceError> {
    let (sql, values) = compile_select(&get_snapshot_by_revision_query(
        &type_naming.document_type_id,
        doc_id,
        revision,
    ));
    let maybe_row = sqlx::query_with(AssertSqlSafe(sql.as_str()), values)
        .fetch_optional(pool)
        .await
        .map_err(PersistenceError::from)?;
    maybe_row
        .map(|row| map_snapshot_row(&row, type_naming))
        .transpose()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
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
    async fn test_sqlx_document_snapshot_repository_new() {
        let (context, naming) = fixture_context_and_naming();
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/test")
            .expect("lazy pool creation must succeed");

        let repo = SqlxDocumentSnapshotRepository::new(pool, context, naming);
        assert_eq!(
            repo.context().config.default_locale,
            context.config.default_locale
        );
    }
}
