use thiserror::Error;
use tracing::error;

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
        match &err {
            sqlx::Error::Database(db_err) => match db_err.code().as_deref() {
                Some("23505") => {
                    SchedulerError::Conflict("数据已存在，请检查是否有重复记录".to_string())
                }
                Some("23503") => SchedulerError::Validation("关联数据不存在或无法删除".to_string()),
                Some("23514") => SchedulerError::Validation(db_err.message().to_string()),
                Some("22P02") => SchedulerError::Validation("数据格式无效".to_string()),
                Some("22023") => SchedulerError::Validation("参数值无效".to_string()),
                Some("08006") | Some("08001") | Some("08004") | Some("57P03") => {
                    SchedulerError::Database("数据库连接异常，请稍后重试".to_string())
                }
                Some("57014") => SchedulerError::Database("数据库操作超时，请稍后重试".to_string()),
                _ => {
                    let err_str = err.to_string();
                    if err_str.contains("invalid cidr") {
                        SchedulerError::Validation("不符合CIDR格式".to_string())
                    } else if err_str.contains("invalid inet") {
                        SchedulerError::Validation("不符合IP地址格式".to_string())
                    } else {
                        error!("数据库错误: {}", err_str);
                        SchedulerError::Database("数据库操作失败，请稍后重试".to_string())
                    }
                }
            },
            sqlx::Error::RowNotFound => SchedulerError::NotFound("资源不存在".to_string()),
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => {
                SchedulerError::Database("数据库连接异常，请稍后重试".to_string())
            }
            sqlx::Error::Io(_) => {
                SchedulerError::Database("数据库连接异常，请稍后重试".to_string())
            }
            _ => {
                error!("数据库错误: {}", err);
                SchedulerError::Database("数据库操作失败，请稍后重试".to_string())
            }
        }
    }
}

pub type SchedulerResult<T> = Result<T, SchedulerError>;
