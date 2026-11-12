//! Database Migration Plan Execution Engine.
//!
//! Executes `MigrationPlan` statements sequentially against PostgreSQL or AWS Aurora DSQL
//! outside transaction blocks.

use sqlx::PgPool;

use super::ddl::step_to_sql;
use super::planner::MigrationPlan;
use crate::persistence::error::PersistenceError;

// ---------------------------------------------------------------------------
// Public Execution Types
// ---------------------------------------------------------------------------

/// Result summary of executing a migration plan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct ExecutionSummary {
    pub(super) executed_statements: Vec<String>,
}

// ---------------------------------------------------------------------------
// Public Execution Entry Point
// ---------------------------------------------------------------------------

/// Executes a `MigrationPlan` statement-by-statement outside transaction blocks.
///
/// # Errors
/// Returns [`PersistenceError`] if any DDL statement fails execution.
pub(super) async fn execute_migration_plan(
    pool: &PgPool,
    plan: &MigrationPlan,
) -> Result<ExecutionSummary, PersistenceError> {
    let mut executed = Vec::with_capacity(plan.len());

    for step in &plan.steps {
        let sql = step_to_sql(step);
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(pool)
            .await?;
        executed.push(sql);
    }

    Ok(ExecutionSummary {
        executed_statements: executed,
    })
}
