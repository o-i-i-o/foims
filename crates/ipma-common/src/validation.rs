//! validator 校验错误到 i18n 消息的映射。
//!
//! 模型校验消息（`#[validate(..., message = "...")]`）已统一使用消息 key，
//! 此处提取第一个错误的 message 作为 key 返回给前端翻译。

use crate::msg::{AppMessage, msg};

/// 提取校验错误中的第一个消息 key。
pub fn validation_errors_to_message(err: &validator::ValidationErrors) -> AppMessage {
    let key = err
        .field_errors()
        .values()
        .next()
        .and_then(|errs| errs.first())
        .and_then(|e| e.message.as_ref().map(|m| m.to_string()))
        .unwrap_or_else(|| "server.error.validation".to_string());
    if key.starts_with("server.") {
        AppMessage::new(key)
    } else {
        // 兼容尚未迁移为 key 的历史消息：包装为通用校验失败并透出原文
        msg("server.error.validation").with("reason", key)
    }
}
