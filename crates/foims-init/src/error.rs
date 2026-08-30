//! 初始化模块错误类型。
//!
//! 所有变体携带 [`AppMessage`]（i18n key + 动态参数），由前端负责翻译。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use foims_common::{ApiResponse, AppMessage, DbErrorKind, msg};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum InitError {
    #[error("数据库错误: {0}")]
    Database(AppMessage),

    #[error("资源未找到: {0}")]
    NotFound(AppMessage),

    #[error("验证失败: {0}")]
    Validation(AppMessage),

    #[error("认证失败: {0}")]
    Unauthorized(AppMessage),

    #[error("权限不足: {0}")]
    Forbidden(AppMessage),

    #[error("冲突: {0}")]
    Conflict(AppMessage),

    #[error("内部错误: {0}")]
    Internal(AppMessage),
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

impl IntoResponse for InitError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = match self {
            InitError::Database(m) => {
                foims_common::log_error!("log.error.database_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.database")))
            }
            InitError::Internal(m) => {
                foims_common::log_error!("log.error.internal_detail", detail = m.log_string());
                Json(ApiResponse::<()>::error(msg("server.error.internal")))
            }
            InitError::NotFound(m)
            | InitError::Validation(m)
            | InitError::Unauthorized(m)
            | InitError::Forbidden(m)
            | InitError::Conflict(m) => Json(ApiResponse::<()>::error(m)),
        };
        (status, body).into_response()
    }
}

/// 构造成功 JSON 响应（委托 foims-common 的统一实现）。
pub use foims_common::ok_json;

impl From<sqlx::Error> for InitError {
    fn from(err: sqlx::Error) -> Self {
        match foims_common::classify_db_error(&err) {
            DbErrorKind::Conflict(m) => InitError::Conflict(m),
            DbErrorKind::Validation(m) => InitError::Validation(m),
            DbErrorKind::NotFound => InitError::NotFound(msg("server.common.not_found")),
            DbErrorKind::Database(m) => InitError::Database(m),
        }
    }
}

impl From<validator::ValidationErrors> for InitError {
    fn from(err: validator::ValidationErrors) -> Self {
        InitError::Validation(foims_common::validation_errors_to_message(&err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use std::borrow::Cow;
    use validator::{ValidationError, ValidationErrors};

    /// 各变体到 HTTP 状态码的映射
    #[test]
    fn status_code_各变体映射() {
        let m = msg("server.x");
        let cases: Vec<(InitError, StatusCode)> = vec![
            (
                InitError::Database(m.clone()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (InitError::NotFound(m.clone()), StatusCode::NOT_FOUND),
            (InitError::Validation(m.clone()), StatusCode::BAD_REQUEST),
            (InitError::Unauthorized(m.clone()), StatusCode::UNAUTHORIZED),
            (InitError::Forbidden(m.clone()), StatusCode::FORBIDDEN),
            (InitError::Conflict(m.clone()), StatusCode::CONFLICT),
            (InitError::Internal(m), StatusCode::INTERNAL_SERVER_ERROR),
        ];
        for (err, expected) in cases {
            assert_eq!(err.status_code(), expected, "变体 {err}");
        }
    }

    /// Display 输出带中文前缀与消息 key
    #[test]
    fn display_包含分类前缀与key() {
        assert_eq!(
            InitError::Validation(msg("server.v")).to_string(),
            "验证失败: server.v"
        );
        assert_eq!(
            InitError::Database(msg("server.d")).to_string(),
            "数据库错误: server.d"
        );
    }

    /// RowNotFound 归类为 404 NotFound
    #[test]
    fn from_sqlx_行不存在映射404() {
        let err = InitError::from(sqlx::Error::RowNotFound);
        match &err {
            InitError::NotFound(m) => assert_eq!(m.key(), "server.common.not_found"),
            other => panic!("应映射为 NotFound，实际 {other}"),
        }
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
    }

    /// 连接池超时归类为 500 数据库错误
    #[test]
    fn from_sqlx_连接池超时映射500() {
        let err = InitError::from(sqlx::Error::PoolTimedOut);
        assert!(matches!(err, InitError::Database(_)));
        assert_eq!(err.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    /// 校验错误集合转换为 Validation 分支并透出首个消息 key
    #[test]
    fn from_validation_errors_透出消息key() {
        let mut field_err = ValidationError::new("length");
        field_err.message = Some(Cow::from("server.init.validation.username_length"));
        let mut errors = ValidationErrors::new();
        errors.add("username", field_err);
        let err = InitError::from(errors);
        match &err {
            InitError::Validation(m) => {
                assert_eq!(m.key(), "server.init.validation.username_length")
            }
            other => panic!("应映射为 Validation，实际 {other}"),
        }
    }

    /// 校验类错误原样透传消息 key
    #[tokio::test]
    async fn into_response_校验错误透传key() {
        let resp =
            InitError::Validation(msg("server.init.validation.username_length")).into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap_or_else(|e| panic!("读取响应体失败: {e}"));
        let body = String::from_utf8_lossy(&bytes);
        assert!(body.contains(r#""success":false"#), "响应体: {body}");
        assert!(
            body.contains(r#""message":"server.init.validation.username_length""#),
            "响应体: {body}"
        );
    }

    /// 数据库错误返回通用 key，不透出内部诊断信息
    #[tokio::test]
    async fn into_response_数据库错误返回通用key() {
        let resp = InitError::Database(msg("server.detail.sensitive")).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap_or_else(|e| panic!("读取响应体失败: {e}"));
        let body = String::from_utf8_lossy(&bytes);
        assert!(
            body.contains(r#""message":"server.error.database""#),
            "响应体: {body}"
        );
        assert!(!body.contains("sensitive"), "不应透出内部详情: {body}");
    }

    /// 内部错误同样仅返回通用 key
    #[tokio::test]
    async fn into_response_内部错误返回通用key() {
        let resp = InitError::Internal(msg("server.detail.internal")).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap_or_else(|e| panic!("读取响应体失败: {e}"));
        let body = String::from_utf8_lossy(&bytes);
        assert!(
            body.contains(r#""message":"server.error.internal""#),
            "响应体: {body}"
        );
    }
}
