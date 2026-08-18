//! PostgreSQL 错误归类映射。
//!
//! 将 sqlx 错误按 PostgreSQL 错误码归类为面向用户的消息 key，供各 crate
//! 错误枚举的 `From<sqlx::Error>` 实现委托调用，保证全项目对同一数据库
//! 错误返回一致的 HTTP 语义与文案 key。

use crate::msg::{AppMessage, msg};
use sqlx::Error as SqlxError;

/// sqlx 错误归类后的统一结果。
#[derive(Debug)]
pub enum DbErrorKind {
    /// 唯一键冲突（PG 23505），对应 409 语义。
    Conflict(AppMessage),
    /// 客户端输入错误：外键约束（23503）、CHECK 约束（23514）、
    /// 数据格式（22P02/22023）及 inet/cidr 解析失败，对应 4xx 语义。
    Validation(AppMessage),
    /// 目标行不存在（RowNotFound），对应 404 语义。
    NotFound,
    /// 基础设施故障：连接异常、超时、连接池故障及未分类数据库错误。
    /// 详细原因已通过 tracing 记录到服务端，仅返回通用消息避免泄露内部信息。
    Database(AppMessage),
}

/// 将 sqlx 错误归类为面向用户的消息 key。
pub fn classify_db_error(err: &SqlxError) -> DbErrorKind {
    match err {
        SqlxError::Database(db_err) => match db_err.code().as_deref() {
            Some("23505") => DbErrorKind::Conflict(msg("server.db.conflict")),
            Some("23503") => DbErrorKind::Validation(msg("server.db.fk_violation")),
            // CHECK 约束消息由 DDL 生成，作为动态参数透出给前端
            Some("23514") => DbErrorKind::Validation(
                msg("server.db.check_violation").with("message", db_err.message()),
            ),
            Some("22P02") => DbErrorKind::Validation(msg("server.db.invalid_format")),
            Some("22023") => DbErrorKind::Validation(msg("server.db.invalid_parameter")),
            Some("08006" | "08001" | "08004" | "57P03") => {
                DbErrorKind::Database(msg("server.db.connection_error"))
            }
            Some("57014") => DbErrorKind::Database(msg("server.db.timeout")),
            _ => {
                let err_str = err.to_string();
                if err_str.contains("invalid cidr") {
                    DbErrorKind::Validation(msg("server.db.invalid_cidr"))
                } else if err_str.contains("invalid inet") {
                    DbErrorKind::Validation(msg("server.db.invalid_inet"))
                } else {
                    crate::log_error!("log.error.database_detail", detail = err_str);
                    DbErrorKind::Database(msg("server.db.operation_failed"))
                }
            }
        },
        SqlxError::RowNotFound => DbErrorKind::NotFound,
        SqlxError::PoolTimedOut | SqlxError::PoolClosed | SqlxError::Io(_) => {
            DbErrorKind::Database(msg("server.db.connection_error"))
        }
        _ => {
            crate::log_error!("log.error.database_detail", detail = err);
            DbErrorKind::Database(msg("server.db.operation_failed"))
        }
    }
}
