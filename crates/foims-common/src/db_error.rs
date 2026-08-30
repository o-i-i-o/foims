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

#[cfg(test)]
mod tests {
    use super::*;

    /// 可注入 SQLSTATE 与消息的数据库错误桩，用于构造 Database 分支
    #[derive(Debug)]
    struct MockDbError {
        message: String,
        code: Option<String>,
    }

    impl std::fmt::Display for MockDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for MockDbError {}

    impl sqlx::error::DatabaseError for MockDbError {
        fn message(&self) -> &str {
            &self.message
        }

        fn code(&self) -> Option<std::borrow::Cow<'_, str>> {
            self.code.as_deref().map(std::borrow::Cow::Borrowed)
        }

        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }

        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }

        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
    }

    /// 构造带 SQLSTATE 的数据库错误
    fn db_err(code: Option<&str>, message: &str) -> SqlxError {
        SqlxError::Database(Box::new(MockDbError {
            message: message.to_string(),
            code: code.map(str::to_string),
        }))
    }

    /// 返回 (分类名, 分类内消息)，便于对各分支统一断言
    fn kind_info(kind: &DbErrorKind) -> (&'static str, AppMessage) {
        match kind {
            DbErrorKind::Conflict(m) => ("conflict", m.clone()),
            DbErrorKind::Validation(m) => ("validation", m.clone()),
            DbErrorKind::NotFound => ("not_found", msg("")),
            DbErrorKind::Database(m) => ("database", m.clone()),
        }
    }

    #[test]
    fn 唯一约束冲突_23505_归为_conflict() {
        let (kind, m) = kind_info(&classify_db_error(&db_err(Some("23505"), "dup key")));
        assert_eq!(kind, "conflict");
        assert_eq!(m.key(), "server.db.conflict");
    }

    #[test]
    fn 外键约束_23503_归为_validation() {
        let (kind, m) = kind_info(&classify_db_error(&db_err(Some("23503"), "fk fails")));
        assert_eq!(kind, "validation");
        assert_eq!(m.key(), "server.db.fk_violation");
    }

    #[test]
    fn check约束_23514_消息作为参数透出() {
        let (kind, m) = kind_info(&classify_db_error(&db_err(
            Some("23514"),
            "new row violates check constraint",
        )));
        assert_eq!(kind, "validation");
        assert_eq!(m.key(), "server.db.check_violation");
        let params = m
            .params_map()
            .unwrap_or_else(|| panic!("应携带 message 参数"));
        assert_eq!(
            params.get("message"),
            Some(&"new row violates check constraint".to_string())
        );
    }

    #[test]
    fn 数据格式错误码_归为_validation() {
        for (code, key) in [
            ("22P02", "server.db.invalid_format"),
            ("22023", "server.db.invalid_parameter"),
        ] {
            let (kind, m) = kind_info(&classify_db_error(&db_err(Some(code), "bad input")));
            assert_eq!(kind, "validation", "SQLSTATE {code}");
            assert_eq!(m.key(), key);
        }
    }

    #[test]
    fn 连接类错误码_归为_database_连接错误() {
        for code in ["08006", "08001", "08004", "57P03"] {
            let (kind, m) = kind_info(&classify_db_error(&db_err(Some(code), "conn err")));
            assert_eq!(kind, "database", "SQLSTATE {code}");
            assert_eq!(m.key(), "server.db.connection_error");
        }
    }

    #[test]
    fn 查询取消_57014_归为_database_超时() {
        let (kind, m) = kind_info(&classify_db_error(&db_err(Some("57014"), "cancel")));
        assert_eq!(kind, "database");
        assert_eq!(m.key(), "server.db.timeout");
    }

    #[test]
    fn 无错误码时按消息识别_cidr与inet解析失败() {
        let (kind, m) = kind_info(&classify_db_error(&db_err(None, "invalid cidr format")));
        assert_eq!(kind, "validation");
        assert_eq!(m.key(), "server.db.invalid_cidr");

        let (kind, m) = kind_info(&classify_db_error(&db_err(None, "invalid inet format")));
        assert_eq!(kind, "validation");
        assert_eq!(m.key(), "server.db.invalid_inet");
    }

    #[test]
    fn 未分类数据库错误_归为_database_通用失败() {
        let (kind, m) = kind_info(&classify_db_error(&db_err(Some("42601"), "syntax error")));
        assert_eq!(kind, "database");
        assert_eq!(m.key(), "server.db.operation_failed");
    }

    #[test]
    fn row_not_found_归为_not_found() {
        let (kind, _) = kind_info(&classify_db_error(&SqlxError::RowNotFound));
        assert_eq!(kind, "not_found");
    }

    #[test]
    fn 连接池与io错误_归为_database_连接错误() {
        let cases = vec![
            SqlxError::PoolTimedOut,
            SqlxError::PoolClosed,
            SqlxError::Io(std::io::Error::other("broken pipe")),
        ];
        for err in cases {
            let (kind, m) = kind_info(&classify_db_error(&err));
            assert_eq!(kind, "database");
            assert_eq!(m.key(), "server.db.connection_error");
        }
    }

    #[test]
    fn 其余变体_归为_database_通用失败() {
        let cases = vec![
            SqlxError::ColumnNotFound("col".to_string()),
            SqlxError::TypeNotFound {
                type_name: "t".to_string(),
            },
            SqlxError::ColumnIndexOutOfBounds { index: 9, len: 2 },
        ];
        for err in cases {
            let (kind, m) = kind_info(&classify_db_error(&err));
            assert_eq!(kind, "database");
            assert_eq!(m.key(), "server.db.operation_failed");
        }
    }
}
