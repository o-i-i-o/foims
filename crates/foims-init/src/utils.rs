//! 初始化辅助工具。

use foims_common::msg;

pub fn url_encode_component(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    result
}

/// 构造 PostgreSQL 连接串：用户名/密码/库名统一 URL 编码，
/// 含 @ : / 等字符时裸拼会导致误报（I-8）；host 保持原样以支持
/// IPv6 字面量（[::1] 形式，与既有各探测端点口径一致）
pub fn build_pg_url(config: &crate::types::DatabaseConfig, database: &str) -> String {
    format!(
        "postgres://{}:{}@{}:{}/{}",
        url_encode_component(&config.username),
        url_encode_component(&config.password),
        config.host,
        config.port,
        url_encode_component(database)
    )
}

/// pgpass 临时文件：复用 foims-common 的唯一定义
pub use foims_common::pgpass::PgPassFile;

pub async fn hash_password(password: &str) -> Result<String, crate::error::InitError> {
    // 防御性校验：bcrypt 仅处理前 72 字节，超长部分被静默截断；
    // 入口（InitRequest）已拦截，此处兜底防止绕过校验的调用路径
    if password.len() > crate::types::PASSWORD_MAX_BYTES {
        return Err(crate::error::InitError::Validation(msg(
            "server.init.validation.password_length",
        )));
    }
    let password = password.to_string();
    tokio::task::spawn_blocking(move || bcrypt::hash(&password, bcrypt::DEFAULT_COST))
        .await
        .map_err(|e| {
            crate::error::InitError::Internal(
                msg("server.init.password_hash_task_failed").with("error", e),
            )
        })?
        .map_err(|err| {
            crate::error::InitError::Internal(
                msg("server.init.password_hash_failed").with("error", err),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url编码_非保留字符原样保留() {
        assert_eq!(url_encode_component("abcXYZ09"), "abcXYZ09");
        assert_eq!(url_encode_component("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(url_encode_component(""), "");
    }

    #[test]
    fn url编码_特殊字符转为百分号大写十六进制() {
        assert_eq!(url_encode_component("a b"), "a%20b");
        assert_eq!(url_encode_component("p@ss:word/?"), "p%40ss%3Aword%2F%3F");
        assert_eq!(url_encode_component("a+b"), "a%2Bb");
        assert_eq!(url_encode_component("100%"), "100%25");
        assert_eq!(url_encode_component("a=b&c=d"), "a%3Db%26c%3Dd");
    }

    #[test]
    fn url编码_多字节字符按utf8字节编码() {
        // “中文” 的 UTF-8 字节为 E4 B8 AD E6 96 87
        assert_eq!(url_encode_component("中文"), "%E4%B8%AD%E6%96%87");
    }

    #[test]
    fn 连接串构造_特殊字符凭据被编码() {
        let cfg = crate::types::DatabaseConfig {
            host: "127.0.0.1".to_string(),
            port: 5432,
            database: "ignored".to_string(),
            username: "u@ser".to_string(),
            password: "p:ss/w".to_string(),
            max_connections: 10,
            min_connections: 5,
            acquire_timeout_secs: 15,
            idle_timeout_secs: 60,
            max_lifetime_secs: 1800,
            query_timeout_secs: 30,
            health_check_interval_secs: 30,
        };
        assert_eq!(
            build_pg_url(&cfg, "postgres"),
            "postgres://u%40ser:p%3Ass%2Fw@127.0.0.1:5432/postgres"
        );
        assert_eq!(
            build_pg_url(&cfg, "my/db"),
            "postgres://u%40ser:p%3Ass%2Fw@127.0.0.1:5432/my%2Fdb"
        );
    }

    #[test]
    fn pgpass文件_写入内容与权限并在drop时清理() {
        // 用户名带进程号与线程号保证并发测试下文件名唯一
        let username = format!("u{}", std::process::id());
        let pgpass = PgPassFile::create("dbhost", 5432, "foims", &username, "p@ss:w0rd")
            .unwrap_or_else(|e| panic!("创建 pgpass 失败: {e}"));

        let path = pgpass.path().to_path_buf();
        assert!(path.exists(), "pgpass 文件应已写入");

        let content =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读取 pgpass 失败: {e}"));
        assert_eq!(content, format!("dbhost:5432:foims:{username}:p@ss:w0rd\n"));

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

    /// 哈希结果应可被 bcrypt 校验（正例与反例）
    #[tokio::test]
    async fn 密码哈希_可校验且带盐() {
        let hash = hash_password("admin123")
            .await
            .unwrap_or_else(|e| panic!("哈希失败: {e}"));
        assert_eq!(hash.len(), 60, "bcrypt 哈希固定 60 字符");
        assert!(hash.starts_with("$2"), "应为 bcrypt 格式");
        assert!(bcrypt::verify("admin123", &hash).unwrap_or(false));
        assert!(!bcrypt::verify("wrong-password", &hash).unwrap_or(true));

        // 两次哈希因随机盐而不同
        let hash2 = hash_password("admin123")
            .await
            .unwrap_or_else(|e| panic!("哈希失败: {e}"));
        assert_ne!(hash, hash2);
    }
}
