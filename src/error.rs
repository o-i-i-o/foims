//! 全局错误类型（AppError）：统一 HTTP 语义并对内部错误脱敏。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ipma_common::{ApiResponse, DbErrorKind};
use thiserror::Error;
use tracing::error;

#[derive(Error, Debug)]
pub enum AppError {
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

    #[error("SNMP错误: {0}")]
    Snmp(String),
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
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = match self {
            AppError::Internal(msg) => {
                error!("内部错误详情: {msg}");
                Json(ApiResponse::<()>::error("服务器内部错误，请稍后重试"))
            }
            AppError::Database(msg) => {
                // 详细错误仅写入服务端日志，避免向客户端泄露数据库结构等内部信息
                error!("数据库错误详情: {msg}");
                Json(ApiResponse::<()>::error("数据库操作失败，请稍后重试"))
            }
            other => Json(ApiResponse::<()>::error(other.to_string())),
        };
        (status, body).into_response()
    }
}

/// 构造成功 JSON 响应（委托 ipma-common 的统一实现）。
pub use ipma_common::ok_json;

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
            DbErrorKind::Conflict(msg) => AppError::Conflict(msg),
            DbErrorKind::Validation(msg) => AppError::Validation(msg),
            DbErrorKind::NotFound => AppError::NotFound("资源不存在".to_string()),
            DbErrorKind::Database(msg) => AppError::Database(msg),
        }
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        AppError::Validation(err.to_string())
    }
}

impl From<ipma_data_manager::DataError> for AppError {
    fn from(err: ipma_data_manager::DataError) -> Self {
        match err {
            ipma_data_manager::DataError::Database(msg) => AppError::Database(msg),
            ipma_data_manager::DataError::NotFound(msg) => AppError::NotFound(msg),
            ipma_data_manager::DataError::Validation(msg) => AppError::Validation(msg),
            ipma_data_manager::DataError::Conflict(msg) => AppError::Conflict(msg),
            ipma_data_manager::DataError::Internal(msg) => AppError::Internal(msg),
        }
    }
}

impl From<ipma_visualization::VisualizationError> for AppError {
    fn from(err: ipma_visualization::VisualizationError) -> Self {
        match err {
            ipma_visualization::VisualizationError::Database(msg) => AppError::Database(msg),
            ipma_visualization::VisualizationError::NotFound(msg) => AppError::NotFound(msg),
            ipma_visualization::VisualizationError::Validation(msg) => AppError::Validation(msg),
            ipma_visualization::VisualizationError::Conflict(msg) => AppError::Conflict(msg),
            ipma_visualization::VisualizationError::Internal(msg) => AppError::Internal(msg),
        }
    }
}

impl From<ipma_scheduler::SchedulerError> for AppError {
    fn from(err: ipma_scheduler::SchedulerError) -> Self {
        match err {
            ipma_scheduler::SchedulerError::Database(msg) => AppError::Database(msg),
            ipma_scheduler::SchedulerError::NotFound(msg) => AppError::NotFound(msg),
            ipma_scheduler::SchedulerError::Validation(msg) => AppError::Validation(msg),
            ipma_scheduler::SchedulerError::Conflict(msg) => AppError::Conflict(msg),
            ipma_scheduler::SchedulerError::TaskNotFound(msg) => AppError::NotFound(msg),
            ipma_scheduler::SchedulerError::Execution(msg) => AppError::Internal(msg),
            ipma_scheduler::SchedulerError::Internal(msg) => AppError::Internal(msg),
        }
    }
}
