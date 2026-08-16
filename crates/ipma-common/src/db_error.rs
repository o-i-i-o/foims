//! PostgreSQL 错误归类映射。
//!
//! 将 sqlx 错误按 PostgreSQL 错误码归类为面向用户的统一消息，供各 crate
//! 错误枚举的 `From<sqlx::Error>` 实现委托调用，保证全项目对同一数据库
//! 错误返回一致的 HTTP 语义与文案。

use sqlx::Error as SqlxError;
use tracing::error;

/// sqlx 错误归类后的统一结果。
#[derive(Debug)]
pub enum DbErrorKind {
    /// 唯一键冲突（PG 23505），对应 409 语义，消息可直接返回客户端。
    Conflict(String),
    /// 客户端输入错误：外键约束（23503）、CHECK 约束（23514）、
    /// 数据格式（22P02/22023）及 inet/cidr 解析失败，对应 4xx 语义。
    Validation(String),
    /// 目标行不存在（RowNotFound），对应 404 语义。
    NotFound,
    /// 基础设施故障：连接异常、超时、连接池故障及未分类数据库错误。
    /// 详细原因已通过 tracing 记录到服务端，仅返回通用消息避免泄露内部信息。
    Database(String),
}

/// 将 sqlx 错误归类为统一的用户可读消息。
pub fn classify_db_error(err: &SqlxError) -> DbErrorKind {
    match err {
        SqlxError::Database(db_err) => match db_err.code().as_deref() {
            Some("23505") => DbErrorKind::Conflict("数据已存在，请检查是否有重复记录".to_string()),
            Some("23503") => DbErrorKind::Validation("关联数据不存在或无法删除".to_string()),
            // CHECK 约束消息本身面向业务，直接透出
            Some("23514") => DbErrorKind::Validation(db_err.message().to_string()),
            Some("22P02") => DbErrorKind::Validation("数据格式无效".to_string()),
            Some("22023") => DbErrorKind::Validation("参数值无效".to_string()),
            Some("08006" | "08001" | "08004" | "57P03") => {
                DbErrorKind::Database("数据库连接异常，请稍后重试".to_string())
            }
            Some("57014") => DbErrorKind::Database("数据库操作超时，请稍后重试".to_string()),
            _ => {
                let err_str = err.to_string();
                if err_str.contains("invalid cidr") {
                    DbErrorKind::Validation("不符合CIDR格式".to_string())
                } else if err_str.contains("invalid inet") {
                    DbErrorKind::Validation("不符合IP地址格式".to_string())
                } else {
                    error!("数据库错误: {err_str}");
                    DbErrorKind::Database("数据库操作失败，请稍后重试".to_string())
                }
            }
        },
        SqlxError::RowNotFound => DbErrorKind::NotFound,
        SqlxError::PoolTimedOut | SqlxError::PoolClosed | SqlxError::Io(_) => {
            DbErrorKind::Database("数据库连接异常，请稍后重试".to_string())
        }
        _ => {
            error!("数据库错误: {err}");
            DbErrorKind::Database("数据库操作失败，请稍后重试".to_string())
        }
    }
}
