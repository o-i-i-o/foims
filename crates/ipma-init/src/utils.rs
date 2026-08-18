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
