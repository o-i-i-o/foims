use std::sync::Arc;

use crate::auth::utils::JwtUtils;
use crate::config::Config;
use crate::crypto::decrypt_password;
use crate::db::DbPool;
use crate::error::AppError;
use ipma_data_manager::{DataError, DataProvider, DataResult, DatabaseConfig};
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
            .ok_or_else(|| AppError::Internal("数据库未初始化".to_string()))
    }
}

impl DataProvider for AppState {
    fn pool(&self) -> DataResult<PgPool> {
        self.pool
            .as_ref()
            .map(|p| p.get_conn())
            .ok_or_else(|| DataError::Internal("数据库未初始化".to_string()))
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

    fn decrypt_password(&self, encrypted: &str) -> DataResult<String> {
        decrypt_password(encrypted).map_err(DataError::Internal)
    }
}
