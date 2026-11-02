//! 通知类型与订阅范围模型：邮件与 Webhook 渠道共用，按类型独立订阅。

use serde::{Deserialize, Serialize};

/// 通知业务类型（三类固定通知，触发点各归属其一）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyKind {
    /// IP 地址变更（含工位 IP 上设备更换）
    IpChange,
    /// 主机资源告警
    AgentAlert,
    /// SNMP 观测事件（未知终端接入等）
    SnmpEvent,
}

/// 通知订阅范围：一个通知接收方（邮件收件人 / Webhook 目标）接收哪些
/// 通知类型。邮件与 Webhook 渠道共用本结构，两渠道彼此独立配置。
/// 未落字段（如存量 JSON 缺项）按启用处理：通知多发的代价远小于静默漏报。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct NotifyScope {
    /// IP 地址变更通知（含工位 IP 上设备更换）开关。
    /// 存量键名 mac_change → ip_change 的重命名迁移由 upgrade SQL 负责（见 upgrade/0.32），
    /// 读取路径只认规范键名，不设 alias 兼容旧键。
    #[serde(rename = "ip_change", default = "default_true")]
    pub ip_change: bool,
    /// 主机资源告警通知开关
    #[serde(default = "default_true")]
    pub agent_alert: bool,
    /// SNMP 观测事件通知开关
    #[serde(default = "default_true")]
    pub snmp_event: bool,
}

impl Default for NotifyScope {
    fn default() -> Self {
        Self {
            ip_change: true,
            agent_alert: true,
            snmp_event: true,
        }
    }
}

impl NotifyScope {
    /// 该订阅范围是否接收指定类型的通知
    pub fn allows(self, kind: NotifyKind) -> bool {
        match kind {
            NotifyKind::IpChange => self.ip_change,
            NotifyKind::AgentAlert => self.agent_alert,
            NotifyKind::SnmpEvent => self.snmp_event,
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 订阅范围缺省全启用() {
        let s = NotifyScope::default();
        assert!(s.ip_change && s.agent_alert && s.snmp_event);
    }

    #[test]
    fn 订阅范围按类型过滤() {
        let scope = NotifyScope {
            ip_change: false,
            agent_alert: true,
            snmp_event: false,
        };
        assert!(!scope.allows(NotifyKind::IpChange));
        assert!(scope.allows(NotifyKind::AgentAlert));
        assert!(!scope.allows(NotifyKind::SnmpEvent));
    }
}
