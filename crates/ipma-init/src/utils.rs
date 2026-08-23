//! 初始化辅助工具。

use ipma_common::{AppMessage, msg};
use std::path::PathBuf;

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

pub struct PgPassFile {
    path: PathBuf,
}

impl PgPassFile {
    pub fn create(
        host: &str,
        port: u16,
        database: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, AppMessage> {
        let pgpass_dir = std::env::temp_dir();
        let pgpass_path = pgpass_dir.join(format!(
            ".pgpass_ipma_{}_{}_{}",
            username,
            database,
            std::process::id()
        ));
        let pgpass_content = format!("{}:{}:{}:{}:{}\n", host, port, database, username, password);
        std::fs::write(&pgpass_path, &pgpass_content)
            .map_err(|e| msg("server.init.db.pgpass_write_failed").with("error", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&pgpass_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| {
                    let _ = std::fs::remove_file(&pgpass_path);
                    msg("server.init.db.pgpass_chmod_failed").with("error", e)
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

pub async fn hash_password(password: &str) -> Result<String, crate::error::InitError> {
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
    fn pgpass文件_写入内容与权限并在drop时清理() {
        // 用户名带进程号与线程号保证并发测试下文件名唯一
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
