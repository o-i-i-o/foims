//! 数据库底层操作（创建/删除/备份/恢复）。

use foims_common::{AppMessage, msg};
use sqlx::PgPool;

use crate::config::get_backup_dir;
use crate::types::DatabaseConfig;
use crate::utils::{PgPassFile, url_encode_component};

/// 校验标识符（数据库名）：非空且仅允许字母、数字和下划线。
pub fn validate_identifier(name: &str) -> Result<(), AppMessage> {
    if name.is_empty() {
        return Err(msg("server.init.db.identifier_empty").with("name", name));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(msg("server.init.db.identifier_invalid").with("name", name));
    }
    Ok(())
}

pub fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

pub async fn backup_database(config: &DatabaseConfig) -> Result<String, AppMessage> {
    let backup_dir = get_backup_dir();
    tokio::fs::create_dir_all(&backup_dir)
        .await
        .map_err(|e| msg("server.init.db.backup_dir_create_failed").with("error", e))?;

    // 备份内容为全库导出（含口令哈希等敏感数据）：目录仅属主可进入，
    // 文件仅属主可读写（unix），防止本机其他用户读取备份
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&backup_dir, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|e| msg("server.init.db.backup_failed").with("error", e))?;
    }

    // 时间戳精度到毫秒：同一秒内的并发备份（初始化向导与手动触发
    // 重叠）不再共用同名文件；同名冲突再按 _1.._99 递增重试
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S%3f");

    let config = config.clone();
    // pg_dump 经 stdout 管道输出：备份文件由程序侧以 create_new + 0600
    // 原子创建并写入，避免"先按 umask(0644) 落盘、成功后才补 chmod"
    // 的暴露窗口；pg_dump 失败时不产生半成品文件
    let output = tokio::task::spawn_blocking(move || {
        let pgpass = PgPassFile::create(
            &config.host,
            config.port,
            &config.database,
            &config.username,
            &config.password,
        )?;

        std::process::Command::new("pg_dump")
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
            .map_err(|e| msg("server.init.db.pg_dump_exec_failed").with("error", e))
    })
    .await
    .map_err(|e| msg("server.init.db.pg_dump_task_failed").with("error", e))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(msg("server.init.db.backup_failed").with("error", stderr));
    }

    let sql_content = output.stdout;
    if sql_content.is_empty() {
        return Err(msg("server.init.db.backup_failed").with("error", "pg_dump 输出为空"));
    }

    // create_new + 0600 写入：文件创建即仅属主可读写；
    // 同一毫秒内并发备份的文件名冲突按 _1.._99 有界递增重试
    //（与 data-management backup.rs 同口径），写入失败时删除残留的
    // 部分文件，不留可读取的半成品备份
    #[cfg(unix)]
    let (backup_file, write_result): (String, std::io::Result<()>) = {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut handle = None;
        let mut candidate = format!("{backup_dir}/foims_backup_{timestamp}.sql");
        for seq in 0..=99u32 {
            if seq > 0 {
                candidate = format!("{backup_dir}/foims_backup_{timestamp}_{seq}.sql");
            }
            match std::fs::OpenOptions::new()
                .mode(0o600)
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => {
                    handle = Some(file);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(msg("server.init.db.backup_failed").with("error", e));
                }
            }
        }
        let Some(mut file) = handle else {
            return Err(
                msg("server.init.db.backup_failed").with("error", "备份文件名冲突超过重试上限")
            );
        };
        (candidate, file.write_all(&sql_content))
    };
    #[cfg(not(unix))]
    let (backup_file, write_result): (String, std::io::Result<()>) = {
        let candidate = format!("{backup_dir}/foims_backup_{timestamp}.sql");
        let result = std::fs::write(&candidate, &sql_content);
        (candidate, result)
    };
    if let Err(e) = write_result {
        let _ = tokio::fs::remove_file(&backup_file).await;
        return Err(msg("server.init.db.backup_failed").with("error", e));
    }

    foims_common::log_info!("log.init.db.backup_created", path = backup_file);
    Ok(backup_file)
}

pub async fn drop_database(config: &DatabaseConfig) -> Result<(), AppMessage> {
    validate_identifier(&config.database)?;

    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        url_encode_component(&config.username),
        url_encode_component(&config.password),
        config.host,
        config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| msg("server.init.db.pgsql_connect_failed").with("error", e))?;

    let terminate_query = r"SELECT pg_terminate_backend(pg_stat_activity.pid)
           FROM pg_stat_activity
           WHERE pg_stat_activity.datname = $1
           AND pid <> pg_backend_pid()";

    sqlx::query(terminate_query)
        .bind(&config.database)
        .execute(&postgres_pool)
        .await
        .map_err(|e| msg("server.init.db.terminate_failed").with("error", e))?;

    foims_common::log_info!("log.init.db.connections_terminated", name = config.database);

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {}",
        quote_ident(&config.database)
    )))
    .execute(&postgres_pool)
    .await
    .map_err(|e| msg("server.init.db.drop_failed").with("error", e))?;

    postgres_pool.close().await;
    foims_common::log_info!("log.init.db.dropped", name = config.database);
    Ok(())
}

pub async fn create_database(config: &DatabaseConfig) -> Result<(), AppMessage> {
    validate_identifier(&config.database)?;

    let postgres_url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        url_encode_component(&config.username),
        url_encode_component(&config.password),
        config.host,
        config.port
    );

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| msg("server.init.db.pgsql_connect_failed").with("error", e))?;

    let db_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&config.database)
            .fetch_one(&postgres_pool)
            .await
            .map_err(|e| msg("server.init.db.check_failed").with("error", e))?;

    if db_exists {
        foims_common::log_info!("log.init.db.exists_skip_create", name = config.database);
        postgres_pool.close().await;
        return Ok(());
    }

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE {} CONNECTION LIMIT = -1",
        quote_ident(&config.database)
    )))
    .execute(&postgres_pool)
    .await
    .map_err(|e| msg("server.init.db.create_failed").with("error", e))?;

    postgres_pool.close().await;
    foims_common::log_info!("log.init.db.created", name = config.database);
    Ok(())
}

pub async fn drop_all_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    // 整段在同一连接上顺序执行：SET session_replication_role 是会话级设置，
    // 若每条语句各自从池中取连接，DROP 可能落在未 SET 的连接上因 FK 失败，
    // 恢复 'origin' 也可能落在别的连接，使池内残留 replica 模式连接、
    // 触发器/FK 对该连接静默失效（security-review D-1）
    let mut conn = pool.acquire().await?;

    let tables: Vec<String> = sqlx::query_scalar(
        r"
        SELECT table_name FROM information_schema.tables
        WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
    ",
    )
    .fetch_all(&mut *conn)
    .await?;

    if !tables.is_empty() {
        sqlx::query("SET session_replication_role = 'replica'")
            .execute(&mut *conn)
            .await?;

        // 逐表 DROP 的结果先挂起：无论成败都必须先把会话复位回 'origin'
        // 再归还连接，避免任一 DROP 失败时连接以 replica 状态回到池中，
        // 后续借用该连接的语句静默跳过 FK 与触发器
        let drop_result: Result<(), sqlx::Error> = async {
            for table in &tables {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "DROP TABLE IF EXISTS {} CASCADE",
                    quote_ident(table)
                )))
                .execute(&mut *conn)
                .await?;
            }
            Ok(())
        }
        .await;

        let reset_result = sqlx::query("SET session_replication_role = 'origin'")
            .execute(&mut *conn)
            .await;

        match (drop_result, reset_result) {
            (Err(e), Err(reset_err)) => {
                // 复位失败同样不可忽略，但 DROP 的原始错误优先返回
                foims_common::log_warn!(
                    "log.init.db.replication_role_reset_failed",
                    error = reset_err
                );
                Err(e)
            }
            (Err(e), Ok(_)) => Err(e),
            (Ok(()), Err(reset_err)) => Err(reset_err),
            (Ok(()), Ok(_)) => Ok(()),
        }
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 标识符校验_合法名称通过() {
        assert!(validate_identifier("foims").is_ok());
        assert!(validate_identifier("db_2024").is_ok());
        assert!(validate_identifier("A1_b").is_ok());
    }

    #[test]
    fn 标识符校验_空名称返回_empty错误() {
        let Err(m) = validate_identifier("") else {
            panic!("空标识符应被拒绝");
        };
        assert_eq!(m.key(), "server.init.db.identifier_empty");
        // 空名称作为 name 参数透出
        let params = m.params();
        assert_eq!(params.len(), 1);
        assert_eq!((params[0].0.as_str(), params[0].1.as_str()), ("name", ""));
    }

    #[test]
    fn 标识符校验_非法字符返回_invalid错误() {
        for name in ["bad-name", "db;DROP", "name with space", "db.name", "db'x"] {
            let Err(m) = validate_identifier(name) else {
                panic!("标识符 {name} 应被拒绝");
            };
            assert_eq!(
                m.key(),
                "server.init.db.identifier_invalid",
                "标识符: {name}"
            );
            let params = m.params();
            assert_eq!(params[0].1.as_str(), name, "非法名称应作为参数透出");
        }
    }

    #[test]
    fn 标识符引用_普通名称加双引号() {
        assert_eq!(quote_ident("foims"), "\"foims\"");
        assert_eq!(quote_ident(""), "\"\"");
    }

    #[test]
    fn 标识符引用_内部双引号翻倍转义() {
        assert_eq!(quote_ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(quote_ident("\""), "\"\"\"\"");
    }
}
