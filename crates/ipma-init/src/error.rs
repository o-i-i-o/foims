//! 初始化模块错误类型。
//!
//! 所有变体携带 [`AppMessage`]（i18n key + 动态参数），由前端负责翻译。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ipma_common::{ApiResponse, AppMessage, DbErrorKind, msg};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InitError {
    #[error("数据库错误: {0}")]
    Database(AppMessage),

    #[error("资源未找到: {0}")]
    NotFound(AppMessage),

    #[error("验证失败: {0}")]
    Validation(AppMessage),

    #[error("认证失败: {0}")]
    Unauthorized(AppMessage),

    #[error("权限不足: {0}")]
    Forbidden(AppMessage),

    #[error("冲突: {0}")]
    Conflict(AppMessage),

    #[error("内部错误: {0}")]
    Internal(AppMessage),
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
        let body = match self {
            InitError::Database(m) => {
                ipma_common::log_error!("log.error.database_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.database")))
            }
            InitError::Internal(m) => {
                ipma_common::log_error!("log.error.internal_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.internal")))
            }
            InitError::NotFound(m)
            | InitError::Validation(m)
            | InitError::Unauthorized(m)
            | InitError::Forbidden(m)
            | InitError::Conflict(m) => Json(ApiResponse::<()>::error(m)),
        };
        (status, body).into_response()
    }
}

/// 构造成功 JSON 响应（委托 ipma-common 的统一实现）。
pub use ipma_common::ok_json;

impl From<sqlx::Error> for InitError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
            DbErrorKind::Conflict(m) => InitError::Conflict(m),
            DbErrorKind::Validation(m) => InitError::Validation(m),
            DbErrorKind::NotFound => InitError::NotFound(msg("server.common.not_found")),
            DbErrorKind::Database(m) => InitError::Database(m),
        }
    }
}

impl From<validator::ValidationErrors> for InitError {
    fn from(err: validator::ValidationErrors) -> Self {
        InitError::Validation(ipma_common::validation_errors_to_message(&err))
    }
}
