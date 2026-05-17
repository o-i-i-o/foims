use actix_web::{HttpResponse, http::StatusCode, ResponseError};
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

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        self.status_code()
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .json(crate::models::ApiResponse::<()>::error(self.to_string()))
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::Database(db_err) => {
                match db_err.code().as_deref() {
                    Some("23505") => AppError::Conflict("数据已存在，请检查是否有重复记录".to_string()),
                    Some("23503") => AppError::Validation("关联数据不存在或无法删除".to_string()),
                    Some("23514") => AppError::Validation(db_err.message().to_string()),
                    Some("22P02") => AppError::Validation("数据格式无效".to_string()),
                    Some("22023") => AppError::Validation("参数值无效".to_string()),
                    Some("08006") | Some("08001") | Some("08004") | Some("57P03") => {
                        AppError::Database("数据库连接异常，请稍后重试".to_string())
                    }
                    Some("57014") => AppError::Database("数据库操作超时，请稍后重试".to_string()),
                    _ => {
                        let err_str = err.to_string();
                        if err_str.contains("invalid cidr") {
                            AppError::Validation("不符合CIDR格式".to_string())
                        } else if err_str.contains("invalid inet") {
                            AppError::Validation("不符合IP地址格式".to_string())
                        } else {
                            error!("数据库错误: {}", err_str);
                            AppError::Database("数据库操作失败，请稍后重试".to_string())
                        }
                    }
                }
            }
            sqlx::Error::RowNotFound => AppError::NotFound("资源不存在".to_string()),
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
                AppError::Database("数据库连接异常，请稍后重试".to_string())
            }
            sqlx::Error::Io(_) => {
                AppError::Database("数据库连接异常，请稍后重试".to_string())
            }
            _ => {
                error!("数据库错误: {}", err);
                AppError::Database("数据库操作失败，请稍后重试".to_string())
            }
        }
    }
}

impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        AppError::Validation(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub trait IntoResponse {
    fn into_response(self) -> HttpResponse;
}

impl<T: serde::Serialize> IntoResponse for AppResult<T> {
    fn into_response(self) -> HttpResponse {
        match self {
            Ok(data) => HttpResponse::Ok().json(crate::models::ApiResponse::success(data, "操作成功")),
            Err(e) => e.error_response(),
        }
    }
}

pub fn not_found<T>(msg: impl Into<String>) -> AppResult<T> {
    Err(AppError::NotFound(msg.into()))
}

pub fn validation<T>(msg: impl Into<String>) -> AppResult<T> {
    Err(AppError::Validation(msg.into()))
}

pub fn conflict<T>(msg: impl Into<String>) -> AppResult<T> {
    Err(AppError::Conflict(msg.into()))
}
