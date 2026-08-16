//! 定时任务错误定义。

use ipma_common::DbErrorKind;
use thiserror::Error;

/// 定时任务模块错误类型。
#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("数据库错误: {0}")]
    Database(String),

    #[error("资源未找到: {0}")]
    NotFound(String),

    #[error("验证失败: {0}")]
    Validation(String),

    #[error("冲突: {0}")]
    Conflict(String),

    #[error("任务未找到: {0}")]
    TaskNotFound(String),

    #[error("任务执行失败: {0}")]
    Execution(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<sqlx::Error> for SchedulerError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
            DbErrorKind::Conflict(msg) => SchedulerError::Conflict(msg),
            DbErrorKind::Validation(msg) => SchedulerError::Validation(msg),
            DbErrorKind::NotFound => SchedulerError::NotFound("资源不存在".to_string()),
            DbErrorKind::Database(msg) => SchedulerError::Database(msg),
        }
    }
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;
