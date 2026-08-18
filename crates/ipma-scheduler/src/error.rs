//! 定时任务错误定义。
//!
//! 所有变体携带 [`AppMessage`]（i18n key + 动态参数），由前端负责翻译。

use ipma_common::{AppMessage, DbErrorKind, msg};
use thiserror::Error;

/// 定时任务模块错误类型。
#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("数据库错误: {0}")]
    Database(AppMessage),

    #[error("资源未找到: {0}")]
    NotFound(AppMessage),

    #[error("验证失败: {0}")]
    Validation(AppMessage),

    #[error("冲突: {0}")]
    Conflict(AppMessage),

    #[error("任务未找到: {0}")]
    TaskNotFound(AppMessage),

    #[error("任务执行失败: {0}")]
    Execution(AppMessage),

    #[error("内部错误: {0}")]
    Internal(AppMessage),
}

impl From<sqlx::Error> for SchedulerError {
    fn from(err: sqlx::Error) -> Self {
        match ipma_common::classify_db_error(&err) {
            DbErrorKind::Conflict(m) => SchedulerError::Conflict(m),
            DbErrorKind::Validation(m) => SchedulerError::Validation(m),
            DbErrorKind::NotFound => SchedulerError::NotFound(msg("server.common.not_found")),
            DbErrorKind::Database(m) => SchedulerError::Database(m),
        }
    }
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;
