//! 全局错误类型（AppError）：统一 HTTP 语义并对内部错误脱敏。
//!
//! 所有变体携带 [`AppMessage`]（i18n key + 动态参数），响应体中的
//! `message` 为消息 key，由前端负责翻译；后端不生成任何用户可见文案。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ipma_common::{ApiResponse, AppMessage, DbErrorKind};
use thiserror::Error;

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
                ipma_common::log_error!("log.error.internal_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.internal")))
            }
            AppError::Database(m) => {
                ipma_common::log_error!("log.error.database_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.database")))
            }
            other => Json(ApiResponse::<()>::error(other.message().clone())),
        };
        (status, body).into_response()
    }
}

/// 构造成功 JSON 响应（委托 ipma-common 的统一实现）。
pub use ipma_common::ok_json;

/// 便捷构造 [`AppMessage`]（`msg("server.xxx.yyy")`）。
pub use ipma_common::msg;

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
            DbErrorKind::Conflict(m) => AppError::Conflict(m),
            DbErrorKind::Validation(m) => AppError::Validation(m),
            DbErrorKind::NotFound => AppError::NotFound(msg("server.common.not_found")),
            DbErrorKind::Database(m) => AppError::Database(m),
        }
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        AppError::Validation(ipma_common::validation_errors_to_message(&err))
    }
}

impl From<ipma_data_manager::DataError> for AppError {
    fn from(err: ipma_data_manager::DataError) -> Self {
        match err {
            ipma_data_manager::DataError::Database(m) => AppError::Database(m),
            ipma_data_manager::DataError::NotFound(m) => AppError::NotFound(m),
            ipma_data_manager::DataError::Validation(m) => AppError::Validation(m),
            ipma_data_manager::DataError::Conflict(m) => AppError::Conflict(m),
            ipma_data_manager::DataError::Internal(m) => AppError::Internal(m),
        }
    }
}

impl From<ipma_visualization::VisualizationError> for AppError {
    fn from(err: ipma_visualization::VisualizationError) -> Self {
        match err {
            ipma_visualization::VisualizationError::Database(m) => AppError::Database(m),
            ipma_visualization::VisualizationError::NotFound(m) => AppError::NotFound(m),
            ipma_visualization::VisualizationError::Validation(m) => AppError::Validation(m),
            ipma_visualization::VisualizationError::Conflict(m) => AppError::Conflict(m),
            ipma_visualization::VisualizationError::Internal(m) => AppError::Internal(m),
        }
    }
}

impl From<ipma_x509_manager::CertManagerError> for AppError {
    fn from(err: ipma_x509_manager::CertManagerError) -> Self {
        match err {
            ipma_x509_manager::CertManagerError::Validation(m) => AppError::Validation(m),
            ipma_x509_manager::CertManagerError::NotFound(m) => AppError::NotFound(m),
            ipma_x509_manager::CertManagerError::Internal(m) => AppError::Internal(m),
        }
    }
}

impl From<ipma_scheduler::SchedulerError> for AppError {
    fn from(err: ipma_scheduler::SchedulerError) -> Self {
        match err {
            ipma_scheduler::SchedulerError::Database(m) => AppError::Database(m),
            ipma_scheduler::SchedulerError::NotFound(m) => AppError::NotFound(m),
            ipma_scheduler::SchedulerError::Validation(m) => AppError::Validation(m),
            ipma_scheduler::SchedulerError::Conflict(m) => AppError::Conflict(m),
            ipma_scheduler::SchedulerError::TaskNotFound(m) => AppError::NotFound(m),
            ipma_scheduler::SchedulerError::Execution(m) => AppError::Internal(m),
            ipma_scheduler::SchedulerError::Internal(m) => AppError::Internal(m),
        }
    }
}
