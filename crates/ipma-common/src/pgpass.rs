//! PostgreSQL 口令文件（pgpass）临时文件管理。
//!
//! `psql`/`pg_dump` 等客户端工具不接收命令行口令（会泄漏进进程列表），
//! 标准做法是经 `PGPASSFILE` 环境变量传递口令文件。本模块提供创建
//! 0600 权限 pgpass 临时文件、`Drop` 时自动清理的 [`PgPassFile`]，
//! 供 `ipma-init` 与 `ipma-data-management` 等调用外部 pg 工具的 crate 复用
//! （原先两处各持一份几乎相同的副本，违反"跨 crate 共享类型放
//! ipma-common"规范）。
//!
//! 错误统一返回 [`AppMessage`]（携带 i18n key），由调用方映射为各自的
//! 错误类型（如 `DataError::Internal` / `InitError::Internal`）。

use std::path::PathBuf;

use rand::RngExt;

use crate::msg;
use crate::{AppMessage, config::DatabaseConfig};

/// 随机后缀长度（字节，十六进制编码后翻倍），避免并发创建同名文件
const RANDOM_SUFFIX_BYTES: usize = 8;

/// 仅保留字母数字，其余字符替换为 `_`（用作文件名可读性前缀）
fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// 生成临时文件随机后缀（项目未把 uuid 列为 ipma-common 依赖，用随机字节替代）
fn random_suffix() -> String {
    let mut bytes = [0u8; RANDOM_SUFFIX_BYTES];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// 0600 权限的临时 pgpass 文件，`Drop` 时自动删除。
///
/// 以 `create_new` 原子创建：避免「先写后 chmod」窗口期内其他本地用户
/// 读取到明文口令（security-review I-6）。
pub struct PgPassFile {
    path: PathBuf,
}

impl PgPassFile {
    /// 创建 pgpass 临时文件，内容为 `host:port:database:username:password` 行。
    pub fn create(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, AppMessage> {
        // 随机后缀避免并发冲突；用户名/库名仅作可读性前缀（转义路径分隔符）
        let pgpass_path = std::env::temp_dir().join(format!(
            ".pgpass_ipma_{}_{}_{}_{}",
            sanitize_component(username),
            sanitize_component(database),
            std::process::id(),
            random_suffix()
        ));
        let pgpass_content = format!("{}:{}:{}:{}:{}\n", host, port, database, username, password);
        // 以 0600 原子创建（create_new）：避免「先写后 chmod」窗口期内
        // 其他本地用户读取到明文口令（security-review I-6）
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .mode(0o600)
                .write(true)
                .create_new(true)
                .open(&pgpass_path)
                .map_err(|e| msg("server.common.pgpass_write_failed").with("error", e))?;
            file.write_all(pgpass_content.as_bytes())
                .map_err(|e| msg("server.common.pgpass_write_failed").with("error", e))?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&pgpass_path, &pgpass_content)
                .map_err(|e| msg("server.common.pgpass_write_failed").with("error", e))?;
        }
        Ok(Self { path: pgpass_path })
    }

    /// 以共享配置中的数据库连接信息创建 pgpass 临时文件。
    pub fn create_for_database(config: &DatabaseConfig) -> Result<Self, AppMessage> {
        Self::create(
            &config.host,
            config.port,
            &config.database,
            &config.username,
            &config.password,
        )
    }

    /// pgpass 文件路径（用于设置 `PGPASSFILE` 环境变量）。
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for PgPassFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_component_仅保留字母数字() {
        assert_eq!(sanitize_component("abcXYZ09"), "abcXYZ09");
        assert_eq!(sanitize_component("a-b/c.d"), "a_b_c_d");
        assert_eq!(sanitize_component(""), "");
    }

    #[test]
    fn random_suffix_为十六进制且长度固定() {
        let suffix = random_suffix();
        assert_eq!(suffix.len(), RANDOM_SUFFIX_BYTES * 2);
        assert!(suffix.bytes().all(|b| b.is_ascii_hexdigit()));
        // 随机性抽查：两次生成不一致
        assert_ne!(suffix, random_suffix());
    }

    #[test]
    fn pgpass文件_写入内容与权限并在drop时清理() {
        // 用户名带进程号保证并发测试下前缀可读性
        let username = format!("u{}", std::process::id());
        let pgpass = PgPassFile::create("dbhost", 5432, "ipma", &username, "p@ss:w0rd")
            .unwrap_or_else(|e| panic!("创建 pgpass 失败: {e}"));

        let path = pgpass.path().to_path_buf();
        assert!(path.exists(), "pgpass 文件应已写入");

        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 pgpass 失败: {e}"));
        assert_eq!(content, format!("dbhost:5432:ipma:{username}:p@ss:w0rd\n"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .unwrap_or_else(|e| panic!("读取元数据失败: {e}"))
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "pgpass 应为仅属主可读写");
        }

        drop(pgpass);
        assert!(!path.exists(), "drop 后应自动删除 pgpass 文件");
    }

    #[test]
    fn pgpass文件_按DatabaseConfig创建() {
        let config = DatabaseConfig {
            host: "h1".to_string(),
            port: 5433,
            database: "db1".to_string(),
            username: "user1".to_string(),
            password: "pw1".to_string(),
            max_connections: 10,
            min_connections: 5,
            acquire_timeout_secs: 15,
            idle_timeout_secs: 60,
            max_lifetime_secs: 1800,
            query_timeout_secs: 30,
            health_check_interval_secs: 30,
        };
        let pgpass = PgPassFile::create_for_database(&config)
            .unwrap_or_else(|e| panic!("按配置创建 pgpass 失败: {e}"));
        let content = std::fs::read_to_string(pgpass.path())
            .unwrap_or_else(|e| panic!("读取 pgpass 失败: {e}"));
        assert_eq!(content, "h1:5433:db1:user1:pw1\n");
    }
}
