use actix_web::{HttpResponse, http::StatusCode};
use std::fmt;
use thiserror::Error;

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
    
    pub fn to_response<T>(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(crate::models::ApiResponse::<T>::error(self.to_string()))
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        let err_str = err.to_string();
        
        if err_str.contains("duplicate key") || err_str.contains("unique constraint") {
            AppError::Conflict("数据已存在，请检查是否有重复记录".to_string())
        } else if err_str.contains("foreign key") {
            AppError::Validation("关联数据不存在或无法删除".to_string())
        } else if err_str.contains("no rows") {
            AppError::NotFound("资源不存在".to_string())
        } else if err_str.contains("connection") || err_str.contains("timeout") {
            AppError::Database("数据库连接异常，请稍后重试".to_string())
        } else if err_str.contains("invalid cidr") {
            AppError::Validation("不符合CIDR格式".to_string())
        } else if err_str.contains("invalid inet") {
            AppError::Validation("不符合IP地址格式".to_string())
        } else {
            AppError::Database(err_str)
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
            Err(e) => e.to_response::<()>(),
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
