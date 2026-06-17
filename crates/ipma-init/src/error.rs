use actix_web::{HttpResponse, ResponseError, http::StatusCode};
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

impl ResponseError for InitError {
    fn status_code(&self) -> StatusCode {
        self.status_code()
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(
            crate::ApiResponse::<()> {
                success: false,
                message: self.to_string(),
                data: None,
            },
        )
    }
}

impl From<sqlx::Error> for InitError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                Some("23505") => InitError::Conflict("数据已存在，请检查是否有重复记录".to_string()),
                Some("23503") => InitError::Validation("关联数据不存在或无法删除".to_string()),
                Some("23514") => InitError::Validation(db_err.message().to_string()),
                Some("22P02") => InitError::Validation("数据格式无效".to_string()),
                Some("22023") => InitError::Validation("参数值无效".to_string()),
                Some("08006") | Some("08001") | Some("08004") | Some("57P03") => {
                    InitError::Database("数据库连接异常，请稍后重试".to_string())
                }
                Some("57014") => InitError::Database("数据库操作超时，请稍后重试".to_string()),
                _ => {
                    let err_str = err.to_string();
                    if err_str.contains("invalid cidr") {
                        InitError::Validation("不符合CIDR格式".to_string())
                    } else if err_str.contains("invalid inet") {
                        InitError::Validation("不符合IP地址格式".to_string())
                    } else {
                        tracing::error!("数据库错误: {}", err_str);
                        InitError::Database("数据库操作失败，请稍后重试".to_string())
                    }
                }
            },
            sqlx::Error::RowNotFound => InitError::NotFound("资源不存在".to_string()),
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
                InitError::Database("数据库连接异常，请稍后重试".to_string())
            }
            sqlx::Error::Io(_) => InitError::Database("数据库连接异常，请稍后重试".to_string()),
            _ => {
                tracing::error!("数据库错误: {}", err);
                InitError::Database("数据库操作失败，请稍后重试".to_string())
            }
        }
    }
}

impl From<validator::ValidationErrors> for InitError {
    fn from(err: validator::ValidationErrors) -> Self {
        InitError::Validation(err.to_string())
    }
}
