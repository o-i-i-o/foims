//! 统一 API 响应结构。

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::msg::AppMessage;

/// 统一 API 响应包装结构。
///
/// 所有 HTTP 接口的成功与失败响应均以此结构返回：`success` 标识结果；
/// `message` 为 i18n 消息 key（不是可直接展示的文案），前端使用
/// `message_params` 插值翻译后展示；`data` 仅在成功时携带业务数据。
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    /// 消息 key，由前端 i18n 库翻译
    pub message: String,
    /// 消息动态参数（`{{name}}` 插值），无参数时省略
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message_params: Option<std::collections::HashMap<String, String>>,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// 构造成功响应。
    pub fn success(data: T, message: impl Into<AppMessage>) -> Self {
        let msg = message.into();
        Self {
            success: true,
            message: msg.key().to_string(),
            message_params: msg.params_map(),
            data: Some(data),
        }
    }

    /// 构造失败响应。
    pub fn error(message: impl Into<AppMessage>) -> Self {
        let msg = message.into();
        Self {
            success: false,
            message: msg.key().to_string(),
            message_params: msg.params_map(),
            data: None,
        }
    }
}

/// 构造 200 状态码的成功 JSON 响应。
pub fn ok_json<T: Serialize>(data: T, message: impl Into<AppMessage>) -> Response {
    (StatusCode::OK, Json(ApiResponse::success(data, message))).into_response()
}
