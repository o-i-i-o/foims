use crate::auth::utils::JwtUtils;
use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppError;

pub struct AppState {
    pub config: Config,
    pub pool: Option<DbPool>,
    pub jwt_utils: JwtUtils,
}

impl AppState {
    pub fn new(config: Config, pool: Option<DbPool>) -> Result<Self, String> {
        let jwt_utils = JwtUtils::new(&config)?;
        Ok(Self {
            config,
            pool,
            jwt_utils,
        })
    }

    pub fn pool(&self) -> Result<&DbPool, AppError> {
        self.pool
            .as_ref()
            .ok_or_else(|| AppError::Internal("数据库未初始化".to_string()))
    }
}
