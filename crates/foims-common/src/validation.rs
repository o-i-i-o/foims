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

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use validator::{ValidationError, ValidationErrors};

    /// 构造仅含单个字段错误的校验错误集
    fn errors_with_message(message: Option<Cow<'static, str>>) -> ValidationErrors {
        let mut err = ValidationError::new("length");
        err.message = message;
        let mut errors = ValidationErrors::new();
        errors.add("username", err);
        errors
    }

    #[test]
    fn 空错误集_回退到通用校验失败_key() {
        let m = validation_errors_to_message(&ValidationErrors::new());
        assert_eq!(m.key(), "server.error.validation");
        assert!(m.params().is_empty(), "回退 key 不应携带参数");
    }

    #[test]
    fn 无消息的错误_回退到通用校验失败_key() {
        let m = validation_errors_to_message(&errors_with_message(None));
        assert_eq!(m.key(), "server.error.validation");
        assert!(m.params().is_empty());
    }

    #[test]
    fn server前缀消息_直接作为_key_不带参数() {
        let m = validation_errors_to_message(&errors_with_message(Some(Cow::from(
            "server.init.validation.username_length",
        ))));
        assert_eq!(m.key(), "server.init.validation.username_length");
        assert!(m.params().is_empty(), "合法 key 不应包装 reason 参数");
    }

    #[test]
    fn 非server前缀消息_包装为_reason_参数() {
        let m = validation_errors_to_message(&errors_with_message(Some(Cow::from(
            "用户名长度必须在 3 到 50 之间",
        ))));
        assert_eq!(m.key(), "server.error.validation");
        let params = m.params();
        assert_eq!(params.len(), 1);
        assert_eq!(
            (params[0].0.as_str(), params[0].1.as_str()),
            ("reason", "用户名长度必须在 3 到 50 之间")
        );
    }
}
