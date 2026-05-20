use sqlx::PgPool;
use tracing::{info, warn};

use crate::db::url_encode_component;
use crate::init::operations::{quote_ident, validate_identifier};

pub async fn ensure_database_and_schema(
    config: &crate::config::DatabaseConfig,
) -> Result<PgPool, String> {
    validate_identifier(&config.database, "数据库名")?;

    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        url_encode_component(&config.username),
        url_encode_component(&config.password),
        config.host,
        config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| format!("连接PostgreSQL失败: {e}"))?;

    let db_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&config.database)
            .fetch_one(&postgres_pool)
            .await
            .map_err(|e| format!("检查数据库是否存在失败: {e}"))?;

    if !db_exists {
        info!("数据库 {} 不存在，正在创建...", config.database);
        sqlx::query(&format!(
            "CREATE DATABASE {}",
            quote_ident(&config.database)
        ))
        .execute(&postgres_pool)
        .await
        .map_err(|e| format!("创建数据库失败: {e}"))?;
        info!("数据库 {} 创建成功", config.database);
    }

    postgres_pool.close().await;

    let db_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        url_encode_component(&config.username),
        url_encode_component(&config.password),
        config.host,
        config.port,
        url_encode_component(&config.database)
    );

    let pool = PgPool::connect(&db_url)
        .await
        .map_err(|e| format!("连接数据库失败: {e}"))?;

    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = 'public')",
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| format!("检查schema是否存在失败: {e}"))?;

    if !schema_exists {
        info!("public schema 不存在，正在创建...");
        sqlx::query("CREATE SCHEMA IF NOT EXISTS public")
            .execute(&pool)
            .await
            .map_err(|e| format!("创建schema失败: {e}"))?;
        if let Err(e) = sqlx::query("GRANT ALL ON SCHEMA public TO postgres")
            .execute(&pool)
            .await
        {
            warn!("设置postgres权限失败: {}", e);
        }
        if let Err(e) = sqlx::query("GRANT ALL ON SCHEMA public TO public")
            .execute(&pool)
            .await
        {
            warn!("设置public权限失败: {}", e);
        }
        info!("public schema 创建成功");
    }

    Ok(pool)
}
