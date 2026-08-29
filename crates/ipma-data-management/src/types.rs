//! 数据管理模块的公共类型、错误定义与数据提供者抽象。

use async_trait::async_trait;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ipma_common::{ApiResponse, AppMessage, DbErrorKind, msg};
use serde::Deserialize;
use sqlx::PgPool;
use thiserror::Error;
use validator::Validate;

/// 成功 JSON 响应构造（由 ipma-common 提供，保持原有路径兼容）。
pub use ipma_common::ok_json;

#[derive(Error, Debug)]
pub enum DataError {
    #[error("数据库错误: {0}")]
    Database(AppMessage),

    #[error("资源未找到: {0}")]
    NotFound(AppMessage),

    #[error("验证失败: {0}")]
    Validation(AppMessage),

    #[error("冲突: {0}")]
    Conflict(AppMessage),

    #[error("内部错误: {0}")]
    Internal(AppMessage),
}

impl DataError {
    /// 提取错误内携带的 i18n 消息（供调用方把 key 与参数透传给前端）。
    pub fn message(&self) -> AppMessage {
        match self {
            DataError::Database(m)
            | DataError::NotFound(m)
            | DataError::Validation(m)
            | DataError::Conflict(m)
            | DataError::Internal(m) => m.clone(),
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            DataError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            DataError::NotFound(_) => StatusCode::NOT_FOUND,
            DataError::Validation(_) => StatusCode::BAD_REQUEST,
            DataError::Conflict(_) => StatusCode::CONFLICT,
            DataError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for DataError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = match self {
            DataError::Database(m) => {
                ipma_common::log_error!("log.error.database_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.database")))
            }
            DataError::Internal(m) => {
                ipma_common::log_error!("log.error.internal_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.internal")))
            }
            DataError::NotFound(m) | DataError::Validation(m) | DataError::Conflict(m) => {
                Json(ApiResponse::<()>::error(m))
            }
        };
        (status, body).into_response()
    }
}

impl From<sqlx::Error> for DataError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
            DbErrorKind::Conflict(m) => DataError::Conflict(m),
            DbErrorKind::Validation(m) => DataError::Validation(m),
            DbErrorKind::NotFound => DataError::NotFound(msg("server.common.not_found")),
            DbErrorKind::Database(m) => DataError::Database(m),
        }
    }
}

impl From<validator::ValidationErrors> for DataError {
    fn from(err: validator::ValidationErrors) -> Self {
        DataError::Validation(ipma_common::validation_errors_to_message(&err))
    }
}

pub type DataResult<T> = Result<T, DataError>;

/// 数据库连接配置：直接复用 ipma-common 的唯一定义
///（原先此处持有的 5 字段子集副本已删除，字段以 common 版为准）
pub use ipma_common::config::DatabaseConfig;

#[async_trait]
pub trait DataProvider: Clone + Send + Sync + 'static {
    fn pool(&self) -> DataResult<PgPool>;
    fn database_config(&self) -> DatabaseConfig;
    async fn decrypt_password(&self, encrypted: &str) -> DataResult<String>;
    /// 将明文凭据加密为本实例密文（导入设备 SNMP 凭据列时使用）。
    async fn encrypt_password(&self, plain: &str) -> DataResult<String>;
}

#[derive(Debug, Deserialize, Validate)]
pub struct ClearLogsRequest {
    #[validate(length(min = 1, max = 20, message = "server.logs.validation.log_type_length"))]
    pub log_type: String,
    pub days: Option<i32>,
}

impl From<DataError> for ipma_common::AppError {
    fn from(err: DataError) -> Self {
        match err {
            DataError::Database(m) => ipma_common::AppError::Database(m),
            DataError::NotFound(m) => ipma_common::AppError::NotFound(m),
            DataError::Validation(m) => ipma_common::AppError::Validation(m),
            DataError::Conflict(m) => ipma_common::AppError::Conflict(m),
            DataError::Internal(m) => ipma_common::AppError::Internal(m),
        }
    }
}
