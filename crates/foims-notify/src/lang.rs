//! 通知语言与双语文案模型：邮件收件人 / Webhook 目标在通知卡片上各自设置
//! 接收语言，业务文案由各生产方以中英双语构造、渠道发送时按语言取值。

use serde::{Deserialize, Serialize};

/// 通知接收语言（存量 JSON 无 `language` 字段时按 serde default 回落中文）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum NotifyLang {
    /// 中文（缺省值：存量配置无 language 字段时按中文处理）
    #[default]
    Zh,
    /// 英文
    En,
}

impl NotifyLang {
    /// 日志参数中的语言标识
    pub fn label(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }
}

/// 中英双语文案：标题/正文各一份（中文保持既有文案，英文为对译，动态参数
/// 两种语言一致），渠道发送时按接收方设置的 [`NotifyLang`] 取值。
#[derive(Debug, Clone)]
pub struct LocalizedText {
    pub zh: String,
    pub en: String,
}

impl LocalizedText {
    /// 按语言取文案
    pub fn pick(&self, lang: NotifyLang) -> &str {
        match lang {
            NotifyLang::Zh => &self.zh,
            NotifyLang::En => &self.en,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 语言_json_小写解析() {
        assert_eq!(
            serde_json::from_str::<NotifyLang>(r#""zh""#)
                .unwrap_or_else(|e| panic!("zh 应可读: {e}")),
            NotifyLang::Zh
        );
        assert_eq!(
            serde_json::from_str::<NotifyLang>(r#""en""#)
                .unwrap_or_else(|e| panic!("en 应可读: {e}")),
            NotifyLang::En
        );
    }

    #[test]
    fn 缺省语言为中文() {
        assert_eq!(NotifyLang::default(), NotifyLang::Zh);
    }

    #[test]
    fn 双语文案按语言取值() {
        let text = LocalizedText {
            zh: "中文".to_string(),
            en: "English".to_string(),
        };
        assert_eq!(text.pick(NotifyLang::Zh), "中文");
        assert_eq!(text.pick(NotifyLang::En), "English");
    }
}
