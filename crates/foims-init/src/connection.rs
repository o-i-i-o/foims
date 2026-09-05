//! 数据库连接建立与 schema 存在性保证。

use foims_common::{AppMessage, msg};
use sqlx::PgPool;

use crate::types::DatabaseConfig;
use crate::utils::build_pg_url;

use crate::operations::{quote_ident, validate_identifier};

pub async fn ensure_database_and_schema(config: &DatabaseConfig) -> Result<PgPool, AppMessage> {
    validate_identifier(&config.database)?;

    let postgres_url = build_pg_url(config, "postgres");

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| msg("server.init.db.pgsql_connect_failed").with("error", e))?;

    let db_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&config.database)
            .fetch_one(&postgres_pool)
            .await
            .map_err(|e| msg("server.init.db.check_failed").with("error", e))?;

    if !db_exists {
        foims_common::log_info!("log.init.db.missing_creating", name = config.database);
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE DATABASE {}",
            quote_ident(&config.database)
        )))
        .execute(&postgres_pool)
        .await
        .map_err(|e| msg("server.init.db.create_failed").with("error", e))?;
        foims_common::log_info!("log.init.db.created", name = config.database);
    }

    postgres_pool.close().await;

    let db_url = build_pg_url(config, &config.database);

    let pool = PgPool::connect(&db_url)
        .await
        .map_err(|e| msg("server.init.db.connect_failed").with("error", e))?;

    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = 'public')",
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| msg("server.init.db.schema_check_failed").with("error", e))?;

    if !schema_exists {
        foims_common::log_info!("log.init.db.schema_missing_creating");
        sqlx::query("CREATE SCHEMA IF NOT EXISTS public")
            .execute(&pool)
            .await
            .map_err(|e| msg("server.init.db.schema_create_failed").with("error", e))?;
        if let Err(e) = sqlx::query("GRANT ALL ON SCHEMA public TO postgres")
            .execute(&pool)
            .await
        {
            foims_common::log_warn!("log.init.db.grant_postgres_failed", error = e);
        }
        if let Err(e) = sqlx::query("GRANT ALL ON SCHEMA public TO public")
            .execute(&pool)
            .await
        {
            foims_common::log_warn!("log.init.db.grant_public_failed", error = e);
        }
        foims_common::log_info!("log.init.db.public_schema_created");
    }

    Ok(pool)
}
