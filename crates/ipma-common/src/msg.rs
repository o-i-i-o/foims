//! 面向前端的国际化消息（i18n key + 动态参数）。
//!
//! 约定：后端不翻译、不保存面向用户的文案，所有返回给前端的消息
//! （成功提示、错误、校验失败、通知等）一律携带固定的消息 key 与
//! 动态参数，由前端 i18n 库负责渲染成对应语言的文本。

use std::collections::HashMap;
use std::fmt::Display;

/// 统一消息结构：`key` 为前端翻译表中的固定键，`params` 为动态插值参数。
///
/// 模板占位符使用 `{{name}}` 形式，与前端 i18n 的插值约定保持一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMessage {
    key: String,
    params: Vec<(String, String)>,
}

impl AppMessage {
    /// 创建无参数消息。
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            params: Vec::new(),
        }
    }

    /// 追加一个动态参数（构建器风格），参数值会被转为字符串。
    pub fn with(mut self, name: impl Display, value: impl Display) -> Self {
        self.params.push((name.to_string(), value.to_string()));
        self
    }

    /// 消息 key。
    pub fn key(&self) -> &str {
        &self.key
    }

    /// 动态参数列表。
    pub fn params(&self) -> &[(String, String)] {
        &self.params
    }

    /// 消息参数映射，参数为空时返回 `None`（序列化时省略该字段）。
    pub fn params_map(&self) -> Option<HashMap<String, String>> {
        if self.params.is_empty() {
            return None;
        }
        Some(self.params.iter().cloned().collect())
    }

    /// 面向服务端日志的诊断串：`key(k1=v1, k2=v2)`。
    pub fn log_string(&self) -> String {
        if self.params.is_empty() {
            return self.key.clone();
        }
        let joined = self
            .params
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}({joined})", self.key)
    }
}

impl From<&str> for AppMessage {
    fn from(key: &str) -> Self {
        Self::new(key)
    }
}

impl From<String> for AppMessage {
    fn from(key: String) -> Self {
        Self::new(key)
    }
}

impl Display for AppMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 仅展示 key：后端不持有文案，日志中以此标识消息
        write!(f, "{}", self.key)
    }
}

/// 便捷构造函数：`msg("server.auth.login_failed")`。
pub fn msg(key: impl Into<String>) -> AppMessage {
    AppMessage::new(key)
}
