use sqlx::PgPool;
use tracing::info;

pub async fn ensure_database_and_schema(config: &crate::config::DatabaseConfig) -> Result<PgPool, String> {
    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.username, config.password, config.host, config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| format!("连接PostgreSQL失败: {}", e))?;

    let db_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)"
    )
    .bind(&config.database)
    .fetch_one(&postgres_pool)
    .await
    .unwrap_or(false);

    if !db_exists {
        info!("数据库 {} 不存在，正在创建...", config.database);
        sqlx::query(&format!("CREATE DATABASE \"{}\"", config.database))
            .execute(&postgres_pool)
            .await
            .map_err(|e| format!("创建数据库失败: {}", e))?;
        info!("数据库 {} 创建成功", config.database);
    }

    drop(postgres_pool);

    let db_url = format!(
        "postgres://{}:{}@{}:{}/{}",
        config.username, config.password, config.host, config.port, config.database
    );

    let pool = PgPool::connect(&db_url)
        .await
        .map_err(|e| format!("连接数据库失败: {}", e))?;

    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = 'public')"
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(false);

    if !schema_exists {
        info!("public schema 不存在，正在创建...");
        sqlx::query("CREATE SCHEMA IF NOT EXISTS public")
            .execute(&pool)
            .await
            .map_err(|e| format!("创建schema失败: {}", e))?;
        sqlx::query("GRANT ALL ON SCHEMA public TO postgres")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("GRANT ALL ON SCHEMA public TO public")
            .execute(&pool)
            .await
            .ok();
        info!("public schema 创建成功");
    }

    Ok(pool)
}
