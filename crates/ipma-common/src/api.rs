//! 统一 API 响应结构。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

/// 统一 API 响应包装结构。
///
/// 所有 HTTP 接口的成功与失败响应均以此结构返回：`success` 标识结果，
/// `message` 为可直接展示给用户的消息，`data` 仅在成功时携带业务数据。
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// 构造成功响应。
    pub fn success(data: T, message: &str) -> Self {
        Self {
            success: true,
            message: message.to_string(),
            data: Some(data),
        }
    }

    /// 构造失败响应。
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            data: None,
        }
    }
}

/// 构造 200 状态码的成功 JSON 响应。
pub fn ok_json<T: Serialize>(data: T, message: &str) -> Response {
    (StatusCode::OK, Json(ApiResponse::success(data, message))).into_response()
}
