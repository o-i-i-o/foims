//! 主机资源告警配置面：全局阈值 + 抖动抑制窗口参数的读写与校验。
//!
//! 阈值存 `system_configs`（config_type='agent', key='alert_thresholds'），全局
//! 一套、按指标可单独停用（留空），另带四个窗口参数（持续时长 / 静默窗口 /
//! 恢复冷却 / 迟滞带）。评估、状态持久化与通知由 [`crate::alert_episode`] 的
//! episode 运行时承担，本模块只做三件事：
//! - 配置的读取、校验与保存（Web API 两端点）；
//! - 全局共用的指标口径（指标名 ↔ 阈值 ↔ 当前值 ↔ 通知正文 key）与通知常量；
//! - 保存阈值后作废运行时缓存并补扫一轮（调低阈值无需等兜底扫描周期）。

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use foims_auth::extractor::AdminOrSecAdminUser;
use foims_common::provider::ConfigProvider;
use foims_common::{AppError, msg};

use crate::alert_episode::AlertProvider;

/// 阈值配置在 system_configs 中的存储位置
const CONFIG_TYPE: &str = "agent";
const CONFIG_KEY: &str = "alert_thresholds";
/// 站内通知类型标识（notifications.notification_type 列）
pub(crate) const NOTIFICATION_TYPE: &str = "agent_alert";
/// 通知标题 i18n key（前端按用户语言翻译，key 见 frontend/src/i18n 文案）
pub(crate) const NOTIFICATION_TITLE_KEY: &str = "server.notification.agent_alert.title";

/// 告警指标名（host_alert_episodes.metric 列）与开告警正文 key 成对定义
pub(crate) const METRIC_CPU: &str = "cpu";
pub(crate) const METRIC_MEM: &str = "mem";
pub(crate) const METRIC_DISK: &str = "disk";
pub(crate) const METRIC_TEMP: &str = "temp";
/// 开告警正文 i18n key（静默窗口内就地累计复用同一条文案，只更新数值与次数）
const BODY_KEY_CPU: &str = "server.notification.agent_alert.body_cpu";
const BODY_KEY_MEM: &str = "server.notification.agent_alert.body_mem";
const BODY_KEY_DISK: &str = "server.notification.agent_alert.body_disk";
const BODY_KEY_TEMP: &str = "server.notification.agent_alert.body_temp";
/// 恢复通知正文 i18n key（与开告警同指标成对，免前端翻译指标名）
const RESOLVED_KEY_CPU: &str = "server.notification.agent_alert.resolved_cpu";
const RESOLVED_KEY_MEM: &str = "server.notification.agent_alert.resolved_mem";
const RESOLVED_KEY_DISK: &str = "server.notification.agent_alert.resolved_disk";
const RESOLVED_KEY_TEMP: &str = "server.notification.agent_alert.resolved_temp";

/// 全局告警阈值配置（None/留空 = 不监控该指标；温度单位 ℃，其余为百分比）。
///
/// 窗口参数（`for_secs`/`quiet_window_secs`/`cooldown_secs`/`hysteresis_pct`）
/// 全局一套，作用于「设备 + 指标」维度的告警 episode，见 [`crate::alert_episode`]。
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct AlertThresholds {
    /// 总开关：关闭时不评估任何指标（既有告警状态保留，重开后不重复通知）
    pub enabled: bool,
    pub cpu_pct: Option<u8>,
    pub mem_pct: Option<u8>,
    pub disk_pct: Option<u8>,
    pub temp_c: Option<u8>,
    /// 持续超标多久才开告警（秒，0 = 首次超标即开）
    #[serde(default = "default_for_secs")]
    pub for_secs: u64,
    /// 静默窗口（秒，窗口内同设备同指标只累计次数、不重复发通知）
    #[serde(default = "default_quiet_window_secs")]
    pub quiet_window_secs: u64,
    /// 恢复冷却（秒，恢复后再次开告警的最短通知间隔）
    #[serde(default = "default_cooldown_secs")]
    pub cooldown_secs: u64,
    /// 回落迟滞带（占阈值的百分比，读数落在「阈值减迟滞带」到「阈值」之间时
    /// 既不算超标也不复位，避免在阈值线上抖动反复开合告警）
    #[serde(default = "default_hysteresis_pct")]
    pub hysteresis_pct: u8,
}

const fn default_for_secs() -> u64 {
    15
}

const fn default_quiet_window_secs() -> u64 {
    300
}

const fn default_cooldown_secs() -> u64 {
    600
}

const fn default_hysteresis_pct() -> u8 {
    5
}

impl Default for AlertThresholds {
    /// 默认禁用告警，窗口参数取保守默认（抖动抑制全部生效）。
    fn default() -> Self {
        Self {
            enabled: false,
            cpu_pct: None,
            mem_pct: None,
            disk_pct: None,
            temp_c: None,
            for_secs: default_for_secs(),
            quiet_window_secs: default_quiet_window_secs(),
            cooldown_secs: default_cooldown_secs(),
            hysteresis_pct: default_hysteresis_pct(),
        }
    }
}

/// 窗口参数上限：持续时长 1 小时，静默窗口与冷却 24 小时，迟滞带 50%。
const MAX_FOR_SECS: u64 = 3_600;
const MAX_QUIET_WINDOW_SECS: u64 = 86_400;
const MAX_HYSTERESIS_PCT: u8 = 50;

/// 校验阈值取值：百分比 1-100、温度 1-200（None = 不监控，合法），窗口参数
/// 各自不超过上限。返回 Err 时携带非法参数描述（供 API 400 响应）。
pub fn validate_thresholds(t: &AlertThresholds) -> Result<(), String> {
    for (name, value) in [
        ("cpu_pct", t.cpu_pct),
        ("mem_pct", t.mem_pct),
        ("disk_pct", t.disk_pct),
    ] {
        if let Some(v) = value
            && !(1..=100).contains(&v)
        {
            return Err(format!("{name} 取值非法: {v}（允许 1-100 或留空）"));
        }
    }
    if let Some(v) = t.temp_c
        && !(1..=200).contains(&v)
    {
        return Err(format!("temp_c 取值非法: {v}（允许 1-200 或留空）"));
    }
    if t.for_secs > MAX_FOR_SECS {
        return Err(format!(
            "for_secs 取值非法: {}（允许 0-{MAX_FOR_SECS}）",
            t.for_secs
        ));
    }
    for (name, value) in [
        ("quiet_window_secs", t.quiet_window_secs),
        ("cooldown_secs", t.cooldown_secs),
    ] {
        if value > MAX_QUIET_WINDOW_SECS {
            return Err(format!(
                "{name} 取值非法: {value}（允许 0-{MAX_QUIET_WINDOW_SECS}）"
            ));
        }
    }
    if t.hysteresis_pct > MAX_HYSTERESIS_PCT {
        return Err(format!(
            "hysteresis_pct 取值非法: {}（允许 0-{MAX_HYSTERESIS_PCT}）",
            t.hysteresis_pct
        ));
    }
    Ok(())
}

/// 单台主机参与评估的指标值（None = 本次无读数，episode 状态保持不动）。
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricValues {
    pub cpu_pct: Option<f64>,
    pub mem_pct: Option<f64>,
    pub disk_pct: Option<f64>,
    pub temp_c: Option<f64>,
}

/// 单指标评估口径：episode metric 名 + 阈值 + 本次读数 + 通知正文 key。
///
/// 开告警与静默窗口内的就地累计共用 `body_key`（文案不变，只刷新数值与累计
/// 次数）；`resolved_key` 成对提供恢复文案，避免把 cpu/mem/disk/temp 原词
/// 当参数丢给前端翻译。
pub(crate) struct MetricSpec {
    pub(crate) metric: &'static str,
    pub(crate) threshold: Option<u8>,
    pub(crate) value: Option<f64>,
    pub(crate) body_key: &'static str,
    pub(crate) resolved_key: &'static str,
}

/// 按固定顺序组装四个指标的评估口径；阈值为 None 的指标由调用方跳过
/// （不监控该指标）。
pub(crate) fn metric_specs(thresholds: &AlertThresholds, values: MetricValues) -> [MetricSpec; 4] {
    [
        MetricSpec {
            metric: METRIC_CPU,
            threshold: thresholds.cpu_pct,
            value: values.cpu_pct,
            body_key: BODY_KEY_CPU,
            resolved_key: RESOLVED_KEY_CPU,
        },
        MetricSpec {
            metric: METRIC_MEM,
            threshold: thresholds.mem_pct,
            value: values.mem_pct,
            body_key: BODY_KEY_MEM,
            resolved_key: RESOLVED_KEY_MEM,
        },
        MetricSpec {
            metric: METRIC_DISK,
            threshold: thresholds.disk_pct,
            value: values.disk_pct,
            body_key: BODY_KEY_DISK,
            resolved_key: RESOLVED_KEY_DISK,
        },
        MetricSpec {
            metric: METRIC_TEMP,
            threshold: thresholds.temp_c,
            value: values.temp_c,
            body_key: BODY_KEY_TEMP,
            resolved_key: RESOLVED_KEY_TEMP,
        },
    ]
}

/// 指标名白名单：把库中/入参文本还原为静态常量，未知值返回 None 由调用方按
/// 脏数据跳过——保证 episode 键上的指标名全局只有四种，不会因脏行累积条目。
pub(crate) fn metric_of(name: &str) -> Option<&'static str> {
    if name == METRIC_CPU {
        Some(METRIC_CPU)
    } else if name == METRIC_MEM {
        Some(METRIC_MEM)
    } else if name == METRIC_DISK {
        Some(METRIC_DISK)
    } else if name == METRIC_TEMP {
        Some(METRIC_TEMP)
    } else {
        None
    }
}

/// 从 system_configs 读取阈值配置：未配置返回默认（禁用）；存量 JSON 损坏时
/// 记错误日志并按禁用处理（告警失效但不阻塞上报入库主流程）；JSON 缺窗口
/// 字段按 serde 默认补齐，不会退化成「不抑制抖动」。
pub(crate) async fn load_thresholds(pool: &sqlx::PgPool) -> AlertThresholds {
    let row = sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = $1 AND key = $2",
    )
    .bind(CONFIG_TYPE)
    .bind(CONFIG_KEY)
    .fetch_optional(pool)
    .await;
    match row {
        Ok(Some(value)) => match serde_json::from_str(&value) {
            Ok(t) => t,
            Err(e) => {
                foims_common::log_error!("log.agent.alert_thresholds_parse_failed", error = e);
                AlertThresholds::default()
            }
        },
        Ok(None) => AlertThresholds::default(),
        Err(e) => {
            foims_common::log_error!("log.agent.alert_thresholds_query_failed", error = e);
            AlertThresholds::default()
        }
    }
}

/// 通知接收人：所有启用状态的管理员（admin/sysadmin/secadmin，用户口径）；按人各一条，已读状态独立。
pub(crate) async fn admin_recipient_ids(pool: &sqlx::PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM users WHERE enabled = TRUE AND role IN ('admin', 'sysadmin', 'secadmin')",
    )
    .fetch_all(pool)
    .await
}

/// GET /api/agents/alert-thresholds：读取当前阈值配置（未配置返回默认禁用值）。
pub async fn get_alert_thresholds<S: ConfigProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
) -> Result<Response, AppError> {
    let thresholds = load_thresholds(&state.pool()?.get_conn()).await;
    Ok(foims_common::ok_json(
        thresholds,
        "server.agent.alert_thresholds_retrieved",
    ))
}

/// PUT /api/agents/alert-thresholds：保存全局阈值并立即按当前快照补扫一轮。
pub async fn put_alert_thresholds<S: ConfigProvider + AlertProvider>(
    State(state): State<Arc<S>>,
    _user: AdminOrSecAdminUser,
    Json(thresholds): Json<AlertThresholds>,
) -> Result<Response, AppError> {
    validate_thresholds(&thresholds)
        .map_err(|e| AppError::Validation(msg("server.common.invalid_param").with("param", e)))?;

    let pool = state.pool()?.get_conn();
    let value = serde_json::to_string(&thresholds)
        .map_err(|e| AppError::Internal(msg("server.system.serialize_failed").with("error", e)))?;
    sqlx::query(
        "INSERT INTO system_configs (config_type, key, value) VALUES ($1, $2, $3)
         ON CONFLICT (config_type, key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(CONFIG_TYPE)
    .bind(CONFIG_KEY)
    .bind(&value)
    .execute(&pool)
    .await
    .map_err(|e| AppError::Database(msg("server.db.operation_failed").with("error", e)))?;

    // 保存成功后作废运行时阈值缓存并异步补扫一轮（fire-and-forget）：
    // 一轮扫描涉及全量在线主机，内联 await 会拖慢本请求；扫描内部失败
    // 已自行记日志，不影响保存结果
    if let Some(runtime) = state.alert_runtime() {
        runtime.invalidate_policy();
        tokio::spawn(async move {
            runtime.sweep_once().await;
        });
    }

    Ok(foims_common::ok_json(
        (),
        "server.agent.alert_thresholds_updated",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 阈值校验_合法值通过() {
        let t = AlertThresholds {
            enabled: true,
            cpu_pct: Some(90),
            mem_pct: None,
            disk_pct: Some(1),
            temp_c: Some(200),
            ..Default::default()
        };
        assert!(validate_thresholds(&t).is_ok());
    }

    #[test]
    fn 阈值校验_越界拒绝() {
        let mk = |cpu: Option<u8>, temp: Option<u8>| AlertThresholds {
            enabled: true,
            cpu_pct: cpu,
            mem_pct: None,
            disk_pct: None,
            temp_c: temp,
            ..Default::default()
        };
        assert!(validate_thresholds(&mk(Some(0), None)).is_err(), "0 应拒绝");
        assert!(validate_thresholds(&mk(Some(101), None)).is_err());
        assert!(validate_thresholds(&mk(None, Some(201))).is_err());
        assert!(validate_thresholds(&mk(None, Some(0))).is_err());
        assert!(validate_thresholds(&mk(Some(100), Some(1))).is_ok());
    }

    #[test]
    fn 窗口参数校验_超限拒绝() {
        let mk = |for_secs, quiet, cooldown, hysteresis| AlertThresholds {
            enabled: true,
            cpu_pct: Some(90),
            for_secs,
            quiet_window_secs: quiet,
            cooldown_secs: cooldown,
            hysteresis_pct: hysteresis,
            ..Default::default()
        };
        assert!(
            validate_thresholds(&mk(0, 0, 0, 0)).is_ok(),
            "0 允许（关闭该项抑制）"
        );
        assert!(validate_thresholds(&mk(3_600, 86_400, 86_400, 50)).is_ok());
        assert!(validate_thresholds(&mk(3_601, 300, 600, 5)).is_err());
        assert!(validate_thresholds(&mk(15, 86_401, 600, 5)).is_err());
        assert!(validate_thresholds(&mk(15, 300, 86_401, 5)).is_err());
        assert!(validate_thresholds(&mk(15, 300, 600, 51)).is_err());
    }

    #[test]
    fn 指标规格_顺序与配对固定() {
        let thresholds = AlertThresholds {
            enabled: true,
            cpu_pct: Some(80),
            mem_pct: None,
            disk_pct: Some(90),
            temp_c: Some(75),
            ..Default::default()
        };
        let values = MetricValues {
            cpu_pct: Some(85.0),
            mem_pct: None,
            disk_pct: Some(40.0),
            temp_c: Some(60.0),
        };
        let specs = metric_specs(&thresholds, values);
        assert_eq!(specs.len(), 4);
        assert_eq!(specs[0].metric, "cpu");
        assert_eq!(specs[0].threshold, Some(80));
        assert_eq!(specs[0].value, Some(85.0));
        assert_eq!(specs[0].body_key, BODY_KEY_CPU);
        assert_eq!(specs[0].resolved_key, RESOLVED_KEY_CPU);
        assert_eq!(specs[1].metric, "mem");
        assert_eq!(specs[1].threshold, None, "阈值未配置的指标仍占位");
        assert_eq!(specs[2].metric, "disk");
        assert_eq!(specs[3].metric, "temp");
        assert_eq!(specs[3].body_key, BODY_KEY_TEMP);
    }

    #[test]
    fn 阈值配置_默认禁用且窗口取保守默认() {
        let t = AlertThresholds::default();
        assert!(!t.enabled);
        assert!(t.cpu_pct.is_none() && t.mem_pct.is_none() && t.disk_pct.is_none());
        assert!(t.temp_c.is_none());
        assert_eq!(t.for_secs, 15);
        assert_eq!(t.quiet_window_secs, 300);
        assert_eq!(t.cooldown_secs, 600);
        assert_eq!(t.hysteresis_pct, 5);
    }

    #[test]
    fn 存量阈值配置_缺窗口字段按默认补齐() {
        // 升级前落库的 JSON 不含窗口字段，反序列化必须落到默认而非 0（0 会失去抖动抑制）
        let t: AlertThresholds = serde_json::from_str(
            r#"{"enabled":true,"cpu_pct":90,"mem_pct":null,"disk_pct":null,"temp_c":null}"#,
        )
        .unwrap_or_else(|e| panic!("存量 JSON 应可解析: {e}"));
        assert!(t.enabled);
        assert_eq!(t.cpu_pct, Some(90));
        assert_eq!(t.for_secs, 15);
        assert_eq!(t.quiet_window_secs, 300);
        assert_eq!(t.cooldown_secs, 600);
        assert_eq!(t.hysteresis_pct, 5);
    }
}
