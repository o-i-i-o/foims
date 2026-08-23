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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::msg;
    use axum::http::StatusCode;

    /// 极简 block_on：响应体为内存数据，忙轮询即可完成，无需异步运行时
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        let mut fut = std::pin::pin!(fut);
        loop {
            if let std::task::Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
                return out;
            }
        }
    }

    /// 提取响应体 JSON 文本
    fn body_string(resp: Response) -> String {
        let Ok(bytes) = block_on(axum::body::to_bytes(resp.into_body(), usize::MAX)) else {
            panic!("读取响应体失败");
        };
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// 成功响应构造：标志、key、data 与空参数映射
    #[test]
    fn success_构造_基础字段() {
        let resp = ApiResponse::success(42, "server.ok");
        assert!(resp.success);
        assert_eq!(resp.message, "server.ok");
        assert_eq!(resp.data, Some(42));
        assert!(resp.message_params.is_none(), "无参数时应为 None");
    }

    /// 失败响应构造：success=false 且 data 为 None
    #[test]
    fn error_构造_基础字段() {
        let resp = ApiResponse::<u32>::error("server.error.x");
        assert!(!resp.success);
        assert_eq!(resp.message, "server.error.x");
        assert!(resp.data.is_none());
        assert!(resp.message_params.is_none());
    }

    /// 带动态参数的消息：params_map 提供参数键值
    #[test]
    fn success_构造_携带参数() {
        let resp =
            ApiResponse::success((), msg("server.ok").with("name", "node1").with("count", 3));
        let params = resp.message_params.as_ref().unwrap_or_else(|| {
            panic!("携带参数的消息 message_params 不应为 None");
        });
        assert_eq!(params.get("name"), Some(&"node1".to_string()));
        assert_eq!(params.get("count"), Some(&"3".to_string()));
    }

    /// ok_json 返回 200 + 紧凑 JSON（无参数时省略 message_params 键）
    #[test]
    fn ok_json_无参数_序列化格式() {
        let resp = ok_json((), "server.ok");
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            body_string(resp),
            r#"{"success":true,"message":"server.ok","data":null}"#
        );
    }

    /// ok_json 携带结构化 data 与单个插值参数的序列化格式
    #[test]
    fn ok_json_带数据与参数_序列化格式() {
        // 单参数保证 HashMap 键序稳定，可做全串精确断言
        #[derive(Serialize)]
        struct Payload {
            id: u32,
            name: &'static str,
        }
        let resp = ok_json(
            Payload {
                id: 7,
                name: "node",
            },
            msg("server.ok").with("name", "node"),
        );
        assert_eq!(
            body_string(resp),
            r#"{"success":true,"message":"server.ok","message_params":{"name":"node"},"data":{"id":7,"name":"node"}}"#
        );
    }

    /// 失败响应 JSON：success=false、无 message_params 时省略该键
    #[test]
    fn error_响应_序列化格式() {
        let resp = axum::Json(ApiResponse::<()>::error("server.error.x")).into_response();
        let body = body_string(resp);
        assert_eq!(
            body,
            r#"{"success":false,"message":"server.error.x","data":null}"#
        );
        assert!(!body.contains("message_params"), "无参数时不应出现该键");
    }

    /// From<&str> / From<String> 均可作为消息入参
    #[test]
    fn message_入参_类型转换() {
        let a = ApiResponse::success(1, "k1");
        let b = ApiResponse::success(1, String::from("k1"));
        let c = ApiResponse::success(1, msg("k1"));
        assert_eq!(a.message, "k1");
        assert_eq!(b.message, "k1");
        assert_eq!(c.message, "k1");
    }
}
