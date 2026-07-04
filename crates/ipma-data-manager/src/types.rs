use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use tracing::error;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T, message: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            data: Some(data),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

#[derive(Error, Debug)]
pub enum DataError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("验证失败: {0}")]
    Validation(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl DataError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            DataError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            DataError::Validation(_) => StatusCode::BAD_REQUEST,
            DataError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl ResponseError for DataError {
    fn status_code(&self) -> StatusCode {
        self.status_code()
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(ApiResponse::<()>::error(self.to_string()))
    }
}

impl From<sqlx::Error> for DataError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                Some("23505") => {
                    DataError::Validation("数据已存在，请检查是否有重复记录".to_string())
                }
                Some("23503") => DataError::Validation("关联数据不存在或无法删除".to_string()),
                Some("23514") => DataError::Validation(db_err.message().to_string()),
                Some("22P02") => DataError::Validation("数据格式无效".to_string()),
                Some("22023") => DataError::Validation("参数值无效".to_string()),
                Some("08006") | Some("08001") | Some("08004") | Some("57P03") => {
                    DataError::Database("数据库连接异常，请稍后重试".to_string())
                }
                Some("57014") => DataError::Database("数据库操作超时，请稍后重试".to_string()),
                _ => {
                    let err_str = err.to_string();
                    if err_str.contains("invalid cidr") {
                        DataError::Validation("不符合CIDR格式".to_string())
                    } else if err_str.contains("invalid inet") {
                        DataError::Validation("不符合IP地址格式".to_string())
                    } else {
                        error!("数据库错误: {}", err_str);
                        DataError::Database("数据库操作失败，请稍后重试".to_string())
                    }
                }
            },
            sqlx::Error::RowNotFound => DataError::Validation("资源不存在".to_string()),
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
                DataError::Database("数据库连接异常，请稍后重试".to_string())
            }
            sqlx::Error::Io(_) => DataError::Database("数据库连接异常，请稍后重试".to_string()),
            _ => {
                error!("数据库错误: {}", err);
                DataError::Database("数据库操作失败，请稍后重试".to_string())
            }
        }
    }
}

impl From<validator::ValidationErrors> for DataError {
    fn from(err: validator::ValidationErrors) -> Self {
        DataError::Validation(err.to_string())
    }
}

pub type DataResult<T> = Result<T, DataError>;

pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

pub trait DataProvider: Clone + Send + Sync + 'static {
    fn pool(&self) -> DataResult<PgPool>;
    fn database_config(&self) -> DatabaseConfig;
    fn decrypt_password(&self, encrypted: &str) -> DataResult<String>;
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub mode: Option<String>,
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
