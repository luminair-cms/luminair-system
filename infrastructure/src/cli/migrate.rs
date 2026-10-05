
/// Executes static system migrations and dynamic document schema synchronization without starting HTTP server.
pub async fn migrate(config: &ServerConfig) -> Result<MigrationSummary, CliError> {
    tracing::info!("Connecting to database pool for migrations...");
    let pool = PgPoolOptions::new()
        .max_connections(config.max_db_connections)
        .connect(&config.database_url)
        .await?;

    tracing::info!("Running static system migrations...");
    run_migrations(&pool).await?;

    tracing::info!(
        "Synchronizing document schemas from '{}'...",
        config.schema_dir.display()
    );
    let sync_result = sync_schemas(&pool, &config.schema_dir, SafetyPolicy::AdditiveOnly).await?;

    tracing::info!(
        "Migrations completed: {} dynamic DDL statements executed.",
        sync_result.executed_statements.len()
    );

    Ok(MigrationSummary {
        static_migrations_applied: true,
        executed_statements: sync_result.executed_statements,
    })
}