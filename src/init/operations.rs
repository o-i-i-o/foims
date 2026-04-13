use sqlx::PgPool;
use tracing::info;

use crate::init::config::get_backup_dir;

pub async fn backup_database(config: &crate::config::DatabaseConfig) -> Result<String, String> {
    let backup_dir = get_backup_dir();
    std::fs::create_dir_all(&backup_dir).map_err(|e| format!("创建备份目录失败: {}", e))?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let backup_file = format!("{}/ipma_backup_{}.sql", backup_dir, timestamp);

    let output = std::process::Command::new("pg_dump")
        .arg("-h")
        .arg(&config.host)
        .arg("-p")
        .arg(config.port.to_string())
        .arg("-U")
        .arg(&config.username)
        .arg("-d")
        .arg(&config.database)
        .arg("-f")
        .arg(&backup_file)
        .env("PGPASSWORD", &config.password)
        .output()
        .map_err(|e| format!("执行pg_dump失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("备份失败: {}", stderr));
    }

    info!("数据库备份成功: {}", backup_file);
    Ok(backup_file)
}

pub async fn drop_database(config: &crate::config::DatabaseConfig) -> Result<(), String> {
    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.username, config.password, config.host, config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| format!("连接PostgreSQL失败: {}", e))?;

    let terminate_query = format!(
        r#"SELECT pg_terminate_backend(pg_stat_activity.pid)
           FROM pg_stat_activity
           WHERE pg_stat_activity.datname = '{}'
           AND pid <> pg_backend_pid()"#,
        config.database
    );

    sqlx::query(&terminate_query)
        .execute(&postgres_pool)
        .await
        .map_err(|e| format!("断开数据库连接失败: {}", e))?;

    info!("已断开所有到数据库 {} 的连接", config.database);

    sqlx::query(&format!("DROP DATABASE IF EXISTS \"{}\"", config.database))
        .execute(&postgres_pool)
        .await
        .map_err(|e| format!("删除数据库失败: {}", e))?;

    info!("数据库 {} 已删除", config.database);
    Ok(())
}

pub async fn create_database(config: &crate::config::DatabaseConfig) -> Result<(), String> {
    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.username, config.password, config.host, config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| format!("连接PostgreSQL失败: {}", e))?;

    let db_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&config.database)
            .fetch_one(&postgres_pool)
            .await
            .map_err(|e| format!("检查数据库是否存在失败: {}", e))?;

    if db_exists {
        info!("数据库 {} 已存在，跳过创建", config.database);
        return Ok(());
    }

    sqlx::query(&format!(
        "CREATE DATABASE \"{}\" CONNECTION LIMIT = -1",
        config.database
    ))
    .execute(&postgres_pool)
    .await
    .map_err(|e| format!("创建数据库失败: {}", e))?;

    info!("数据库 {} 创建成功", config.database);
    Ok(())
}

pub async fn drop_all_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let tables: Vec<String> = sqlx::query_scalar::<_, String>(
        r#"
        SELECT table_name FROM information_schema.tables 
        WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
    "#,
    )
    .fetch_all(pool)
    .await?;

    if !tables.is_empty() {
        sqlx::query("SET session_replication_role = 'replica'")
            .execute(pool)
            .await?;

        for table in tables {
            sqlx::query(&format!("DROP TABLE IF EXISTS {} CASCADE", table))
                .execute(pool)
                .await?;
        }

        sqlx::query("SET session_replication_role = 'origin'")
            .execute(pool)
            .await?;
    }

    Ok(())
}
