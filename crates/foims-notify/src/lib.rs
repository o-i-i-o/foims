//! FOIMS 通知-推送域 crate：站内通知原语、SMTP 邮件渠道、Webhook 渠道
//! （通用/钉钉/企业微信）、订阅范围过滤与双渠道外发门面。
//!
//! 依赖方向约束：仅依赖 foims-common，不得依赖任何业务域 crate
//! （foims-auth / foims-snmp / foims-resource / foims-agent-service 反向依赖
//! 本 crate）。业务事件的文案组装（IP 变更/主机告警/未知设备等）留在各
//! 生产方 crate，本 crate 只提供渠道能力与订阅模型，避免膨胀为业务杂集。

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod dispatch;
pub mod in_site;
pub mod kind;
pub mod lang;
pub mod smtp;
pub mod webhook;

pub use dispatch::push;
pub use in_site::{active_admin_ids, create_notification, encode_notification_content};
pub use kind::{NotifyKind, NotifyScope};
pub use lang::{LocalizedText, NotifyLang};
