//! 初始化模块错误类型。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ipma_common::{ApiResponse, DbErrorKind};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InitError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("资源未找到: {0}")]
    NotFound(String),

    #[error("验证失败: {0}")]
    Validation(String),

    #[error("认证失败: {0}")]
    Unauthorized(String),

    #[error("权限不足: {0}")]
    Forbidden(String),

    #[error("冲突: {0}")]
    Conflict(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl InitError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            InitError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            InitError::NotFound(_) => StatusCode::NOT_FOUND,
            InitError::Validation(_) => StatusCode::BAD_REQUEST,
            InitError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            InitError::Forbidden(_) => StatusCode::FORBIDDEN,
            InitError::Conflict(_) => StatusCode::CONFLICT,
            InitError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for InitError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ApiResponse::<()>::error(self.to_string()));
        (status, body).into_response()
    }
}

/// 构造成功 JSON 响应（委托 ipma-common 的统一实现）。
pub use ipma_common::ok_json;

impl From<sqlx::Error> for InitError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
            DbErrorKind::Conflict(msg) => InitError::Conflict(msg),
            DbErrorKind::Validation(msg) => InitError::Validation(msg),
            DbErrorKind::NotFound => InitError::NotFound("资源不存在".to_string()),
            DbErrorKind::Database(msg) => InitError::Database(msg),
        }
    }
}

impl From<validator::ValidationErrors> for InitError {
    fn from(err: validator::ValidationErrors) -> Self {
        InitError::Validation(err.to_string())
    }
}
