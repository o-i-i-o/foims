//! 数据库备份与恢复。

use crate::types::{DataError, DataProvider, DataResult};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use foims_common::{log_error, log_info, log_warn, msg};

/// 执行 pg_dump 并返回原始 SQL 字节
pub fn pg_dump_raw(config: &crate::types::DatabaseConfig) -> DataResult<Vec<u8>> {
    let pgpass = foims_common::pgpass::PgPassFile::create(
        &config.host,
        config.port,
        &config.database,
        &config.username,
        &config.password,
    )
    .map_err(DataError::Internal)?;

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
            DataError::Internal(msg("server.backup.pg_dump_exec_failed").with("error", e))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DataError::Internal(
            msg("server.backup.pg_dump_failed").with("error", stderr),
        ));
    }

    let sql_content = output.stdout;
    if sql_content.is_empty() {
        return Err(DataError::Internal(msg("server.backup.dump_empty")));
    }

    Ok(sql_content)
}

/// 执行 pg_dump 并将结果写入文件，返回备份文件路径
pub fn backup_to_file(
    config: &crate::types::DatabaseConfig,
    backup_dir: &str,
    file_prefix: &str,
) -> DataResult<String> {
    std::fs::create_dir_all(backup_dir).map_err(|e| {
        DataError::Internal(msg("server.backup.dir_create_failed").with("error", e))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(backup_dir, std::fs::Permissions::from_mode(0o700))
        {
            log_warn!(
                "log.backup.dir_permission_failed",
                path = backup_dir,
                error = e
            );
        }
    }

    // 时间戳精度到毫秒：手动备份与定时备份并发落在同一秒时，
    // 秒级文件名会在 create_new 上直接冲突报错
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S%3f").to_string();

    let sql_content = pg_dump_raw(config)?;

    // 以 0600 原子创建写入（I-6）：备份含全库数据，避免先写后 chmod 的暴露窗口。
    // create_new 冲突（同一毫秒内并发备份）时递增序号 `_1`..`_99` 有界重试
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut handle = None;
        let mut backup_file = format!("{backup_dir}/{file_prefix}_{timestamp}.sql");
        for seq in 0..=99u32 {
            if seq > 0 {
                backup_file = format!("{backup_dir}/{file_prefix}_{timestamp}_{seq}.sql");
            }
            match std::fs::OpenOptions::new()
                .mode(0o600)
                .write(true)
                .create_new(true)
                .open(&backup_file)
            {
                Ok(file) => {
                    handle = Some(file);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(DataError::Internal(
                        msg("server.backup.file_write_failed").with("error", e),
                    ));
                }
            }
        }
        let Some(mut file) = handle else {
            return Err(DataError::Internal(
                msg("server.backup.file_write_failed").with("error", "备份文件名冲突超过重试上限"),
            ));
        };
        file.write_all(&sql_content).map_err(|e| {
            DataError::Internal(msg("server.backup.file_write_failed").with("error", e))
        })?;
        Ok(backup_file)
    }
    #[cfg(not(unix))]
    {
        let backup_file = format!("{backup_dir}/{file_prefix}_{timestamp}.sql");
        std::fs::write(&backup_file, &sql_content).map_err(|e| {
            DataError::Internal(msg("server.backup.file_write_failed").with("error", e))
        })?;
        Ok(backup_file)
    }
}

/// 清理超过 keep_days 天的旧备份文件
pub fn cleanup_old_backup_files(backup_dir: &str, keep_days: u64) -> DataResult<()> {
    let entries = std::fs::read_dir(backup_dir)
        .map_err(|e| DataError::Internal(msg("server.backup.dir_read_failed").with("error", e)))?;

    let now = std::time::SystemTime::now();
    let cutoff = std::time::Duration::from_secs(keep_days * 24 * 60 * 60);

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(filename) = path.file_name().and_then(|f| f.to_str())
            && (filename.starts_with("foims_backup_")
                || filename.starts_with("foims_manual_backup_"))
            && filename.ends_with(".sql")
            && let Ok(metadata) = entry.metadata()
            && let Ok(modified) = metadata.modified()
            && let Ok(age) = now.duration_since(modified)
            && age > cutoff
        {
            if let Err(e) = std::fs::remove_file(&path) {
                log_error!(
                    "log.cleanup.old_backup_delete_failed",
                    path = path.display(),
                    error = e
                );
            } else {
                log_info!("log.cleanup.old_backup_deleted", path = path.display());
            }
        }
    }

    Ok(())
}

/// 导出数据库为 HTTP 响应（API 端点使用）
pub async fn export_database<P: DataProvider>(provider: P) -> DataResult<Response> {
    let db_config = provider.database_config();

    let sql_content = tokio::task::spawn_blocking(move || pg_dump_raw(&db_config))
        .await
        .map_err(|e| {
            DataError::Internal(msg("server.backup.pg_dump_task_failed").with("error", e))
        })??;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/sql".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=foims_backup_{}.sql",
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                ),
            ),
        ],
        sql_content,
    )
        .into_response())
}
