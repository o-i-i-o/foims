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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_包含分类前缀与消息key() {
        assert_eq!(
            SchedulerError::Validation(msg("server.v")).to_string(),
            "验证失败: server.v"
        );
        assert_eq!(
            SchedulerError::TaskNotFound(msg("server.t")).to_string(),
            "任务未找到: server.t"
        );
        assert_eq!(
            SchedulerError::Execution(msg("server.e")).to_string(),
            "任务执行失败: server.e"
        );
    }

    #[test]
    fn from_sqlx_行不存在映射为not_found() {
        let err = SchedulerError::from(sqlx::Error::RowNotFound);
        match &err {
            SchedulerError::NotFound(m) => assert_eq!(m.key(), "server.common.not_found"),
            other => panic!("应映射为 NotFound，实际 {other}"),
        }
    }

    #[test]
    fn from_sqlx_连接池超时映射为数据库错误() {
        let err = SchedulerError::from(sqlx::Error::PoolTimedOut);
        assert!(matches!(err, SchedulerError::Database(_)));
    }

    #[test]
    fn from_sqlx_列缺失映射为数据库错误() {
        let err = SchedulerError::from(sqlx::Error::ColumnNotFound("c".to_string()));
        match &err {
            SchedulerError::Database(m) => {
                assert_eq!(m.key(), "server.db.operation_failed");
            }
            other => panic!("应映射为 Database，实际 {other}"),
        }
    }
}
