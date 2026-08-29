//! 全局错误类型（AppError）：统一 HTTP 语义并对内部错误脱敏。
//!
//! 所有变体携带 [`AppMessage`](crate::AppMessage)（i18n key + 动态参数），
//! 响应体中的 `message` 为消息 key，由前端负责翻译；后端不生成任何用户可见文案。
//!
//! 各业务 crate 的专属错误类型通过在本 crate 内实现
//! `From<XxxError> for AppError` 接入统一响应（孤儿规则允许：
//! 实现位于自定义错误类型所在 crate）。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

use crate::{ApiResponse, AppMessage, DbErrorKind, msg};

#[derive(Error, Debug)]
pub enum AppError {
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

    #[error("SNMP错误: {0}")]
    Snmp(AppMessage),
}

impl AppError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Snmp(_) => StatusCode::BAD_REQUEST,
        }
    }

    /// 提取错误内携带的 i18n 消息。
    pub fn message(&self) -> &AppMessage {
        match self {
            AppError::Database(m)
            | AppError::NotFound(m)
            | AppError::Validation(m)
            | AppError::Unauthorized(m)
            | AppError::Forbidden(m)
            | AppError::Conflict(m)
            | AppError::Internal(m)
            | AppError::Snmp(m) => m,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = match self {
            AppError::Internal(m) => {
                // 详细错误仅写入服务端日志，避免向客户端泄露内部信息
                crate::log_error!("log.error.internal_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.internal")))
            }
            AppError::Database(m) => {
                crate::log_error!("log.error.database_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.database")))
            }
            other => Json(ApiResponse::<()>::error(other.message().clone())),
        };
        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match crate::classify_db_error(&err) {
            DbErrorKind::Conflict(m) => AppError::Conflict(m),
            DbErrorKind::Validation(m) => AppError::Validation(m),
            DbErrorKind::NotFound => AppError::NotFound(msg("server.common.not_found")),
            DbErrorKind::Database(m) => AppError::Database(m),
        }
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        AppError::Validation(crate::validation_errors_to_message(&err))
    }
}
