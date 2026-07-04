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

pub async fn export_database<P: DataProvider>(provider: P) -> DataResult<HttpResponse> {
    let db_config = provider.database_config();

    let output = tokio::task::spawn_blocking(move || {
        let pgpass = PgPassFile::create(
            &db_config.host,
            db_config.port,
            &db_config.database,
            &db_config.username,
            &db_config.password,
        )?;

        std::process::Command::new("pg_dump")
            .arg("-h")
            .arg(&db_config.host)
            .arg("-p")
            .arg(db_config.port.to_string())
            .arg("-U")
            .arg(&db_config.username)
            .arg("-d")
            .arg(&db_config.database)
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
            })
    })
    .await
    .map_err(|e| DataError::Internal(format!("pg_dump 任务失败: {e}")))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DataError::Internal(format!("pg_dump 执行失败: {stderr}")));
    }

    let sql_content = output.stdout;
    if sql_content.is_empty() {
        return Err(DataError::Internal("导出的 SQL 文件为空".to_string()));
    }

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
