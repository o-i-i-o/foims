use crate::config::Config;
use crate::db::DbPool;
use crate::error::AppError;

pub struct AppState {
    pub config: Config,
    pub pool: Option<DbPool>,
}

impl AppState {
    pub fn pool(&self) -> Result<&DbPool, AppError> {
        self.pool
            .as_ref()
            .ok_or_else(|| AppError::Internal("数据库未初始化".to_string()))
    }
}
