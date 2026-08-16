//! 数据管理模块的公共类型、错误定义与数据提供者抽象。

use async_trait::async_trait;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ipma_common::{ApiResponse, DbErrorKind};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use validator::Validate;

/// 成功 JSON 响应构造（由 ipma-common 提供，保持原有路径兼容）。
pub use ipma_common::ok_json;

#[derive(Error, Debug)]
pub enum DataError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("资源未找到: {0}")]
    NotFound(String),

    #[error("验证失败: {0}")]
    Validation(String),

    #[error("冲突: {0}")]
    Conflict(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl DataError {
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
        let body = Json(ApiResponse::<()>::error(self.to_string()));
        (status, body).into_response()
    }
}

impl From<sqlx::Error> for DataError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
            DbErrorKind::Conflict(msg) => DataError::Conflict(msg),
            DbErrorKind::Validation(msg) => DataError::Validation(msg),
            DbErrorKind::NotFound => DataError::NotFound("资源不存在".to_string()),
            DbErrorKind::Database(msg) => DataError::Database(msg),
        }
    }
}

impl From<validator::ValidationErrors> for DataError {
    fn from(err: validator::ValidationErrors) -> Self {
        DataError::Validation(err.to_string())
    }
}

pub type DataResult<T> = Result<T, DataError>;

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

#[async_trait]
pub trait DataProvider: Clone + Send + Sync + 'static {
    fn pool(&self) -> DataResult<PgPool>;
    fn database_config(&self) -> DatabaseConfig;
    async fn decrypt_password(&self, encrypted: &str) -> DataResult<String>;
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub success: bool,
    pub message: String,
    pub details: Vec<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ClearLogsRequest {
    #[validate(length(min = 1, max = 20, message = "日志类型长度必须在1到20个字符之间"))]
    pub log_type: String,
    pub days: Option<i32>,
}
