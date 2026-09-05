//! 数据库底层操作（创建/删除/备份/恢复）。

use foims_common::{AppMessage, msg};
use sqlx::PgPool;

use crate::config::get_backup_dir;
use crate::types::DatabaseConfig;
use crate::utils::{PgPassFile, build_pg_url};

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

/// 数据库连接失败分类：面向用户的处置指引不同，配置页据此反馈。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectFailureKind {
    /// 服务器不可达：地址、端口或防火墙问题
    Unreachable,
    /// 认证被拒：用户不存在或密码错误
    AuthRejected,
    /// 目标数据库不存在或无权访问
    DatabaseMissing,
    /// 其他未归类错误
    Other,
}

impl ConnectFailureKind {
    /// 回传前端的消息 key（错误详情只入日志，I-7）
    #[must_use]
    pub fn message_key(self) -> &'static str {
        match self {
            ConnectFailureKind::Unreachable => "server.init.db.test.unreachable",
            ConnectFailureKind::AuthRejected => "server.init.db.test.auth_failed",
            ConnectFailureKind::DatabaseMissing => "server.init.db.test.database_missing",
            ConnectFailureKind::Other => "server.init.db.test.failed",
        }
    }
}

/// 按错误文本归类连接失败（PostgreSQL 标准错误消息为英文，
/// 以小写匹配保证大小写不敏感）。
///
/// 单独抽出文本分类便于用标准错误消息样本做单测：
/// sqlx 的 Database 错误变体无法在测试中直接构造。
fn classify_connect_text(text: &str) -> ConnectFailureKind {
    let text = text.to_ascii_lowercase();
    // "does not exist" 类：先区分角色（认证类）与数据库（库缺失类）
    if text.contains("does not exist") {
        if text.contains("database") {
            return ConnectFailureKind::DatabaseMissing;
        }
        if text.contains("role") {
            return ConnectFailureKind::AuthRejected;
        }
    }
    // 认证类：密码错误 / SCRAM 握手失败 / pg_hba 拒绝
    if text.contains("authentication") || text.contains("password") || text.contains("pg_hba") {
        return ConnectFailureKind::AuthRejected;
    }
    // 不可达类：连接拒绝 / 超时 / DNS 解析失败 / 网络不可达 / 连接被重置
    if text.contains("refused")
        || text.contains("timeout")
        || text.contains("timed out")
        || text.contains("lookup")
        || text.contains("unreachable")
        || text.contains("reset")
        || text.contains("broken pipe")
    {
        return ConnectFailureKind::Unreachable;
    }
    ConnectFailureKind::Other
}

/// 归类 sqlx 连接错误（连接串含凭据，调用方只可把分类结果回传客户端）
#[must_use]
pub fn classify_pg_connect_error(error: &sqlx::Error) -> ConnectFailureKind {
    classify_connect_text(&error.to_string())
}

/// CREATE DATABASE 失败分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateDbFailureKind {
    /// 当前用户无 CREATEDB 权限
    NoPrivilege,
    /// 目标库已被并发创建（查询时不存在、创建时已存在）
    AlreadyExists,
    Other,
}

impl CreateDbFailureKind {
    /// 回传前端的消息 key
    #[must_use]
    pub fn message_key(self) -> &'static str {
        match self {
            CreateDbFailureKind::NoPrivilege => "server.init.db.provision.no_privilege",
            CreateDbFailureKind::AlreadyExists => "server.init.db.provision.exists",
            CreateDbFailureKind::Other => "server.init.db.create_failed",
        }
    }
}

#[must_use]
pub fn classify_create_db_error(error: &sqlx::Error) -> CreateDbFailureKind {
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("permission denied") || text.contains("not permitted") {
        return CreateDbFailureKind::NoPrivilege;
    }
    if text.contains("already exists") {
        return CreateDbFailureKind::AlreadyExists;
    }
    CreateDbFailureKind::Other
}

/// create_database 的结果：区分「本次新建」与「已存在跳过」，
/// 供配置页向用户反馈不同文案
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateOutcome {
    Created,
    AlreadyExists,
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

    let postgres_pool = PgPool::connect(&build_pg_url(config, "postgres"))
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

/// 创建目标数据库（不触碰实例上的其他数据库）。
///
/// - 已存在时幂等跳过（`AlreadyExists`）；
/// - 连接 postgres 系统库失败按 [`classify_pg_connect_error`] 分类，
///   CREATE DATABASE 失败按 [`classify_create_db_error`] 分类，
///   配页页据此给出精确反馈。
pub async fn create_database(config: &DatabaseConfig) -> Result<CreateOutcome, AppMessage> {
    validate_identifier(&config.database)?;

    let postgres_pool = PgPool::connect(&build_pg_url(config, "postgres"))
        .await
        .map_err(|e| {
            let kind = classify_pg_connect_error(&e);
            msg(kind.message_key()).with("error", e)
        })?;

    let db_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&config.database)
            .fetch_one(&postgres_pool)
            .await
            .map_err(|e| msg("server.init.db.check_failed").with("error", e))?;

    if db_exists {
        foims_common::log_info!("log.init.db.exists_skip_create", name = config.database);
        postgres_pool.close().await;
        return Ok(CreateOutcome::AlreadyExists);
    }

    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE DATABASE {} CONNECTION LIMIT = -1",
        quote_ident(&config.database)
    )))
    .execute(&postgres_pool)
    .await
    .map_err(|e| {
        // AlreadyExists 是「查询时不存在、创建时被并发创建」的竞态：
        // 幂等语义下与 AlreadyExists 等价
        let kind = classify_create_db_error(&e);
        if kind == CreateDbFailureKind::AlreadyExists {
            foims_common::log_warn!("log.init.db.create_race_exists", error = e);
        }
        msg(kind.message_key()).with("error", e)
    })?;

    postgres_pool.close().await;
    foims_common::log_info!("log.init.db.created", name = config.database);
    Ok(CreateOutcome::Created)
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

    /// 连接错误文本按 PostgreSQL 标准消息样本归类
    #[test]
    fn 连接错误分类_标准消息样本() {
        // 认证类：密码错误 / 角色不存在 / pg_hba 拒绝
        for text in [
            "error returned from database: password authentication failed for user \"foims\"",
            "FATAL: password authentication failed for user \"foims\"",
            "FATAL: role \"foims\" does not exist",
            "FATAL: no pg_hba.conf entry for host \"127.0.0.1\", user \"foims\", database \"postgres\"",
        ] {
            assert_eq!(
                classify_connect_text(text),
                ConnectFailureKind::AuthRejected,
                "文本: {text}"
            );
        }

        // 库缺失类
        for text in [
            "FATAL: database \"foims\" does not exist",
            "error returned from database: database \"foims_prod\" does not exist",
        ] {
            assert_eq!(
                classify_connect_text(text),
                ConnectFailureKind::DatabaseMissing,
                "文本: {text}"
            );
        }

        // 不可达类：拒绝 / 超时 / DNS / 网络不可达 / 连接重置
        for text in [
            "Connection refused (os error 111)",
            "io error: connection reset by peer",
            "timeout: connection timed out",
            "failed to lookup address information: Name or service not known",
            "Network is unreachable (os error 101)",
            "broken pipe",
        ] {
            assert_eq!(
                classify_connect_text(text),
                ConnectFailureKind::Unreachable,
                "文本: {text}"
            );
        }

        // 未归类
        assert_eq!(
            classify_connect_text("some unexpected failure"),
            ConnectFailureKind::Other
        );

        // 大小写不敏感
        assert_eq!(
            classify_connect_text("FATAL: PASSWORD AUTHENTICATION FAILED"),
            ConnectFailureKind::AuthRejected
        );
    }

    /// sqlx::Error::Io 经 classify_pg_connect_error 同样正确归类
    #[test]
    fn 连接错误分类_io错误归类不可达() {
        let io_err = std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "Connection refused (os error 111)",
        );
        assert_eq!(
            classify_pg_connect_error(&sqlx::Error::Io(io_err)),
            ConnectFailureKind::Unreachable
        );
    }

    /// 各分类对应固定的回传消息 key
    #[test]
    fn 连接错误分类_消息key映射() {
        assert_eq!(
            ConnectFailureKind::Unreachable.message_key(),
            "server.init.db.test.unreachable"
        );
        assert_eq!(
            ConnectFailureKind::AuthRejected.message_key(),
            "server.init.db.test.auth_failed"
        );
        assert_eq!(
            ConnectFailureKind::DatabaseMissing.message_key(),
            "server.init.db.test.database_missing"
        );
        assert_eq!(
            ConnectFailureKind::Other.message_key(),
            "server.init.db.test.failed"
        );
    }

    /// CREATE DATABASE 失败文本按标准消息样本归类
    #[test]
    fn 建库错误分类_标准消息样本() {
        let io_err = std::io::Error::other("ERROR: permission denied to create database");
        assert_eq!(
            classify_create_db_error(&sqlx::Error::Io(io_err)),
            CreateDbFailureKind::NoPrivilege
        );

        let io_err = std::io::Error::other("ERROR: database \"foims\" already exists");
        assert_eq!(
            classify_create_db_error(&sqlx::Error::Io(io_err)),
            CreateDbFailureKind::AlreadyExists
        );

        let io_err = std::io::Error::other("ERROR: source database is being accessed");
        assert_eq!(
            classify_create_db_error(&sqlx::Error::Io(io_err)),
            CreateDbFailureKind::Other
        );

        assert_eq!(
            CreateDbFailureKind::NoPrivilege.message_key(),
            "server.init.db.provision.no_privilege"
        );
        assert_eq!(
            CreateDbFailureKind::AlreadyExists.message_key(),
            "server.init.db.provision.exists"
        );
        assert_eq!(
            CreateDbFailureKind::Other.message_key(),
            "server.init.db.create_failed"
        );
    }
}
