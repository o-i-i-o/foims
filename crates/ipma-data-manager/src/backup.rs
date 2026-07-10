use crate::types::{DataError, DataProvider, DataResult};
use actix_web::HttpResponse;

pub struct PgPassFile {
    path: std::path::PathBuf,
}

impl PgPassFile {
    pub fn create(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
    ) -> DataResult<Self> {
        let pgpass_dir = std::env::temp_dir();
        let pgpass_path = pgpass_dir.join(format!(
            ".pgpass_ipma_{}_{}_{}",
            username,
            database,
            std::process::id()
        ));
        let pgpass_content = format!("{}:{}:{}:{}:{}\n", host, port, database, username, password);
        std::fs::write(&pgpass_path, &pgpass_content)
            .map_err(|e| DataError::Internal(format!("写入 .pgpass 文件失败: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&pgpass_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| {
                    let _ = std::fs::remove_file(&pgpass_path);
                    DataError::Internal(format!("设置 .pgpass 权限失败: {e}"))
                })?;
        }
        Ok(Self { path: pgpass_path })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for PgPassFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 执行 pg_dump 并返回原始 SQL 字节
pub fn pg_dump_raw(config: &crate::types::DatabaseConfig) -> DataResult<Vec<u8>> {
    let pgpass = PgPassFile::create(
        &config.host,
        config.port,
        &config.database,
        &config.username,
        &config.password,
    )?;

    let output = std::process::Command::new("pg_dump")
        .arg("-h")
        .arg(&config.host)
        .arg("-p")
        .arg(config.port.to_string())
        .arg("-U")
        .arg(&config.username)
        .arg("-d")
        .arg(&config.database)
        .arg("--no-owner")
        .arg("--no-acl")
        .arg("--clean")
        .arg("--if-exists")
        .env("PGPASSFILE", pgpass.path())
        .output()
        .map_err(|e| {
            DataError::Internal(format!(
                "执行 pg_dump 失败: {e}。请确保系统已安装 postgresql-client。"
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DataError::Internal(format!("pg_dump 执行失败: {stderr}")));
    }

    let sql_content = output.stdout;
    if sql_content.is_empty() {
        return Err(DataError::Internal("导出的 SQL 文件为空".to_string()));
    }

    Ok(sql_content)
}

/// 执行 pg_dump 并将结果写入文件，返回备份文件路径
pub fn backup_to_file(
    config: &crate::types::DatabaseConfig,
    backup_dir: &str,
    file_prefix: &str,
) -> DataResult<String> {
    std::fs::create_dir_all(backup_dir)
        .map_err(|e| DataError::Internal(format!("创建备份目录失败: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(backup_dir, std::fs::Permissions::from_mode(0o700))
        {
            tracing::warn!("设置备份目录权限失败 {}: {}", backup_dir, e);
        }
    }

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_file = format!("{backup_dir}/{file_prefix}_{timestamp}.sql");

    let sql_content = pg_dump_raw(config)?;

    std::fs::write(&backup_file, sql_content)
        .map_err(|e| DataError::Internal(format!("写入备份文件失败: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&backup_file, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| DataError::Internal(format!("设置备份文件权限失败: {e}")))?;
    }

    Ok(backup_file)
}

/// 清理超过 keep_days 天的旧备份文件
pub fn cleanup_old_backup_files(backup_dir: &str, keep_days: u64) -> DataResult<()> {
    let entries = std::fs::read_dir(backup_dir)
        .map_err(|e| DataError::Internal(format!("读取备份目录失败: {e}")))?;

    let now = std::time::SystemTime::now();
    let cutoff = std::time::Duration::from_secs(keep_days * 24 * 60 * 60);

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(filename) = path.file_name().and_then(|f| f.to_str())
            && (filename.starts_with("ipma_backup_") || filename.starts_with("ipma_manual_backup_"))
            && filename.ends_with(".sql")
            && let Ok(metadata) = entry.metadata()
            && let Ok(modified) = metadata.modified()
            && let Ok(age) = now.duration_since(modified)
            && age > cutoff
        {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::error!("删除旧备份文件失败: {} - {}", path.display(), e);
            } else {
                tracing::info!("删除旧备份文件: {}", path.display());
            }
        }
    }

    Ok(())
}

/// 导出数据库为 HTTP 响应（API 端点使用）
pub async fn export_database<P: DataProvider>(provider: P) -> DataResult<HttpResponse> {
    let db_config = provider.database_config();

    let sql_content = tokio::task::spawn_blocking(move || pg_dump_raw(&db_config))
        .await
        .map_err(|e| DataError::Internal(format!("pg_dump 任务失败: {e}")))??;

    Ok(HttpResponse::Ok()
        .content_type("application/sql")
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=ipma_backup_{}.sql",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            ),
        ))
        .body(sql_content))
}
