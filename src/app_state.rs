//! 应用共享状态（数据库连接池、配置、JWT 工具等）。

use std::sync::Arc;

use async_trait::async_trait;

use crate::auth::utils::JwtUtils;
use crate::config::Config;
use crate::crypto::{decrypt_password_async, encrypt_password_async};
use crate::db::DbPool;
use ipma_common::{AppError, msg};
use ipma_data_management::{DataError, DataProvider, DataResult, DatabaseConfig};
use ipma_scheduler::TaskRegistry;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: Option<DbPool>,
    pub jwt_utils: JwtUtils,
    pub task_registry: Arc<TaskRegistry>,
}

impl AppState {
    pub fn new(
        config: Config,
        pool: Option<DbPool>,
        task_registry: Arc<TaskRegistry>,
    ) -> Result<Self, String> {
        let jwt_utils = JwtUtils::new(&config)?;
        Ok(Self {
            config,
            pool,
            jwt_utils,
            task_registry,
        })
    }

    pub fn pool(&self) -> Result<&DbPool, AppError> {
        self.pool
            .as_ref()
            .ok_or_else(|| AppError::Internal(msg("server.db.not_initialized")))
    }
}

#[async_trait]
impl DataProvider for AppState {
    fn pool(&self) -> DataResult<PgPool> {
        self.pool
            .as_ref()
            .map(|p| p.get_conn())
            .ok_or_else(|| DataError::Internal(msg("server.db.not_initialized")))
    }

    fn database_config(&self) -> DatabaseConfig {
        DatabaseConfig {
            host: self.config.database.host.clone(),
            port: self.config.database.port,
            database: self.config.database.database.clone(),
            username: self.config.database.username.clone(),
            password: self.config.database.password.clone(),
        }
    }

    async fn decrypt_password(&self, encrypted: &str) -> DataResult<String> {
        decrypt_password_async(encrypted.to_string())
            .await
            .map_err(|e| DataError::Internal(msg("server.common.decrypt_failed").with("error", e)))
    }

    async fn encrypt_password(&self, plain: &str) -> DataResult<String> {
        encrypt_password_async(plain.to_string())
            .await
            .map_err(|e| DataError::Internal(msg("server.common.encrypt_failed").with("error", e)))
    }
}
