//! 主机资源告警：全局阈值配置 + 状态翻转评估 + 站内通知。
//!
//! 阈值存 `system_configs`（config_type='agent', key='alert_thresholds'），
//! 全局一套、按指标可单独停用（留空）。评估点两处：
//! - [`crate::ingest`] 入库成功后按本条快照评估（实时性）；
//! - PUT 保存阈值后对全部 active agent 的快照列评估一轮（调低阈值立即生效）。
//!
//! 通知策略：边沿触发——`agent_alert_states.alerting` false→true 时向全部
//! 启用状态管理员（admin/sysadmin/secadmin）发一条站内通知；true→false
//! （回落或读数缺失）静默复位，再次超标才再发。单台主机各指标状态独立。

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use foims_auth::extractor::AdminOrSecAdminUser;
use foims_common::provider::ConfigProvider;
use foims_common::{AppError, msg};

use foims_resource::helpers::create_notification;

/// 阈值配置在 system_configs 中的存储位置
const CONFIG_TYPE: &str = "agent";
const CONFIG_KEY: &str = "alert_thresholds";
/// 站内通知类型标识（notifications.notification_type 列）
const NOTIFICATION_TYPE: &str = "agent_alert";
/// 通知标题 i18n key（前端按用户语言翻译，key 见 web/static/i18n JSON）
const NOTIFICATION_TITLE_KEY: &str = "server.notification.agent_alert.title";

/// 告警指标名（agent_alert_states.metric 列）与通知正文 key 成对定义
const METRIC_CPU: &str = "cpu";
const METRIC_MEM: &str = "mem";
const METRIC_DISK: &str = "disk";
const METRIC_TEMP: &str = "temp";
const BODY_KEY_CPU: &str = "server.notification.agent_alert.body_cpu";
const BODY_KEY_MEM: &str = "server.notification.agent_alert.body_mem";
const BODY_KEY_DISK: &str = "server.notification.agent_alert.body_disk";
const BODY_KEY_TEMP: &str = "server.notification.agent_alert.body_temp";

/// 全局告警阈值配置（None/留空 = 不监控该指标；温度单位 ℃，其余为百分比）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// 总开关：关闭时不评估任何指标（既有告警状态保留，重开后不重复通知）
    pub enabled: bool,
    pub cpu_pct: Option<u8>,
    pub mem_pct: Option<u8>,
    pub disk_pct: Option<u8>,
    pub temp_c: Option<u8>,
}

/// 校验阈值取值：百分比 1-100、温度 1-200（None = 不监控，合法）。
/// 返回 Err 时携带非法参数描述（供 API 400 响应）。
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
    Ok(())
}

/// 单台主机参与评估的指标值（None = 本次无读数，视为未超标并复位告警态）。
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricValues {
    pub cpu_pct: Option<f64>,
    pub mem_pct: Option<f64>,
    pub disk_pct: Option<f64>,
    pub temp_c: Option<f64>,
}

/// 按固定顺序组装指标规格：(状态表 metric 名, 阈值, 当前值, 通知正文 key)。
fn metric_specs(
    thresholds: &AlertThresholds,
    values: MetricValues,
) -> [(&'static str, Option<u8>, Option<f64>, &'static str); 4] {
    [
        (METRIC_CPU, thresholds.cpu_pct, values.cpu_pct, BODY_KEY_CPU),
        (METRIC_MEM, thresholds.mem_pct, values.mem_pct, BODY_KEY_MEM),
        (
            METRIC_DISK,
            thresholds.disk_pct,
            values.disk_pct,
            BODY_KEY_DISK,
        ),
        (METRIC_TEMP, thresholds.temp_c, values.temp_c, BODY_KEY_TEMP),
    ]
}

/// 从 system_configs 读取阈值配置：未配置返回默认（禁用）；存量 JSON 损坏时
/// 记错误日志并按禁用处理（告警失效但不阻塞上报入库主流程）。
pub async fn load_thresholds(pool: &sqlx::PgPool) -> AlertThresholds {
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

/// 通知接收人：所有启用状态的管理员（admin/sysadmin/secadmin，用户口径）。
async fn admin_recipient_ids(pool: &sqlx::PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM users WHERE status = TRUE AND role IN ('admin', 'sysadmin', 'secadmin')",
    )
    .fetch_all(pool)
    .await
}

/// 评估单台主机：对四个指标分别做状态翻转检测，false→true 先发通知再落
/// 状态（通知失败则状态不翻转，下次上报重试，宁可重复不遗漏）；true→false
/// 仅静默复位。返回新增告警的指标数。
async fn evaluate_agent(
    pool: &sqlx::PgPool,
    agent_id: Uuid,
    hostname: &str,
    ip: &str,
    values: MetricValues,
    thresholds: &AlertThresholds,
    admin_ids: &[Uuid],
) -> Result<usize, sqlx::Error> {
    let existing: Vec<(String, bool)> =
        sqlx::query_as("SELECT metric, alerting FROM agent_alert_states WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_all(pool)
            .await?;
    let states: std::collections::HashMap<String, bool> = existing.into_iter().collect();

    let mut newly_alerted = 0usize;
    for (metric, threshold, value, body_key) in metric_specs(thresholds, values) {
        let Some(threshold) = threshold else {
            continue;
        };
        // 超标判定：有读数且严格大于阈值（值缺失视为未超标并复位）
        let exceeding = value.is_some_and(|v| v > f64::from(threshold));
        if states.get(metric).copied().unwrap_or(false) == exceeding {
            continue;
        }
        if exceeding {
            let event = AlertEvent {
                hostname,
                ip,
                metric,
                body_key,
                value: value.unwrap_or_default(),
                threshold,
            };
            notify_admins(pool, admin_ids, &event).await?;
            newly_alerted += 1;
        }
        sqlx::query(
            r"INSERT INTO agent_alert_states (agent_id, metric, alerting)
               VALUES ($1, $2, $3)
               ON CONFLICT (agent_id, metric) DO UPDATE SET alerting = EXCLUDED.alerting",
        )
        .bind(agent_id)
        .bind(metric)
        .bind(exceeding)
        .execute(pool)
        .await?;
    }
    Ok(newly_alerted)
}

/// 单条告警事件上下文：主机标识 + 指标详情（聚合参数避免长参数列表）。
struct AlertEvent<'a> {
    hostname: &'a str,
    ip: &'a str,
    /// 指标名（agent_alert_states.metric）
    metric: &'a str,
    /// 通知正文 i18n key
    body_key: &'a str,
    value: f64,
    threshold: u8,
}

/// 向全部管理员写一条指标超标通知（单条失败即整体失败，由调用方保持
/// 状态不翻转以待重试；标题/正文为前端翻译 key，见 web/static/i18n JSON）。
async fn notify_admins(
    pool: &sqlx::PgPool,
    admin_ids: &[Uuid],
    event: &AlertEvent<'_>,
) -> Result<(), sqlx::Error> {
    let content = serde_json::json!({
        "key": event.body_key,
        "params": {
            "hostname": event.hostname,
            "ip": event.ip,
            "metric": event.metric,
            "value": format!("{:.1}", event.value),
            "threshold": event.threshold.to_string(),
        }
    })
    .to_string();
    for admin_id in admin_ids {
        create_notification(
            pool,
            NOTIFICATION_TITLE_KEY,
            &content,
            NOTIFICATION_TYPE,
            Some(admin_id),
        )
        .await?;
    }
    foims_common::log_info!(
        "log.agent.alert_notification_created",
        hostname = event.hostname,
        metric = event.metric,
        recipients = admin_ids.len()
    );
    Ok(())
}

/// ingest 路径评估入口：读全局阈值（禁用即跳过）→ 查收件人 → 评估单台。
pub async fn evaluate_report(
    pool: &sqlx::PgPool,
    agent_id: Uuid,
    hostname: &str,
    ip: &str,
    values: MetricValues,
) -> Result<(), sqlx::Error> {
    let thresholds = load_thresholds(pool).await;
    if !thresholds.enabled {
        return Ok(());
    }
    let admin_ids = admin_recipient_ids(pool).await?;
    evaluate_agent(
        pool,
        agent_id,
        hostname,
        ip,
        values,
        &thresholds,
        &admin_ids,
    )
    .await?;
    Ok(())
}

/// 保存阈值后对全部在线 agent 的快照指标评估一轮（调低阈值立即触发告警，
/// 仅覆盖 source='agent' 且 active 的主机；SNMP 设备指标二期待接入）。
/// 内部失败逐台记日志，不向调用方传播（保存动作已成功，评估是尽力而为）。
pub async fn evaluate_all(pool: &sqlx::PgPool) {
    let thresholds = load_thresholds(pool).await;
    if !thresholds.enabled {
        return;
    }
    let admin_ids = match admin_recipient_ids(pool).await {
        Ok(ids) => ids,
        Err(e) => {
            foims_common::log_error!("log.agent.alert_recipients_query_failed", error = e);
            return;
        }
    };
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Option<String>,
            Option<String>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
        ),
    >(
        "SELECT id, hostname, ip, cpu_usage::float8, mem_usage_pct::float8, \
             disk_usage_pct::float8, max_temp::float8 \
             FROM agents WHERE source = 'agent' AND status = 'active'",
    )
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(rows) => rows,
        Err(e) => {
            foims_common::log_error!("log.agent.alert_evaluate_failed", error = e);
            return;
        }
    };

    let mut notified = 0usize;
    for (agent_id, hostname, ip, cpu, mem, disk, temp) in rows {
        let values = MetricValues {
            cpu_pct: cpu,
            mem_pct: mem,
            disk_pct: disk,
            temp_c: temp,
        };
        let hostname = hostname.as_deref().unwrap_or("-");
        let ip = ip.as_deref().unwrap_or("-");
        match evaluate_agent(
            pool,
            agent_id,
            hostname,
            ip,
            values,
            &thresholds,
            &admin_ids,
        )
        .await
        {
            Ok(n) => notified += n,
            Err(e) => {
                foims_common::log_error!(
                    "log.agent.alert_evaluate_failed",
                    agent_id = agent_id,
                    error = e
                );
            }
        }
    }
    if notified > 0 {
        foims_common::log_info!("log.agent.alert_evaluate_completed", notified = notified);
    }
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

/// PUT /api/agents/alert-thresholds：保存全局阈值并立即按当前快照评估一轮。
pub async fn put_alert_thresholds<S: ConfigProvider>(
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

    // 保存成功后异步评估一轮（fire-and-forget）：评估是涉及全量在线主机的
    // 长操作，内联 await 会拖慢本请求；内部失败已自行记日志，不影响保存结果
    tokio::spawn(async move {
        evaluate_all(&pool).await;
    });

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
        };
        assert!(validate_thresholds(&mk(Some(0), None)).is_err(), "0 应拒绝");
        assert!(validate_thresholds(&mk(Some(101), None)).is_err());
        assert!(validate_thresholds(&mk(None, Some(201))).is_err());
        assert!(validate_thresholds(&mk(None, Some(0))).is_err());
        assert!(validate_thresholds(&mk(Some(100), Some(1))).is_ok());
    }

    #[test]
    fn 指标规格_顺序与配对固定() {
        let thresholds = AlertThresholds {
            enabled: true,
            cpu_pct: Some(80),
            mem_pct: None,
            disk_pct: Some(90),
            temp_c: Some(75),
        };
        let values = MetricValues {
            cpu_pct: Some(85.0),
            mem_pct: None,
            disk_pct: Some(40.0),
            temp_c: Some(60.0),
        };
        let specs = metric_specs(&thresholds, values);
        assert_eq!(specs.len(), 4);
        assert_eq!(specs[0].0, "cpu");
        assert_eq!(specs[0].1, Some(80));
        assert_eq!(specs[0].2, Some(85.0));
        assert_eq!(specs[1].0, "mem");
        assert_eq!(specs[1].1, None, "阈值未配置的指标仍占位");
        assert_eq!(specs[2].0, "disk");
        assert_eq!(specs[3].0, "temp");
        assert_eq!(specs[3].3, BODY_KEY_TEMP);
    }

    #[test]
    fn 阈值配置_默认禁用且无指标() {
        let t = AlertThresholds::default();
        assert!(!t.enabled);
        assert!(t.cpu_pct.is_none() && t.mem_pct.is_none() && t.disk_pct.is_none());
        assert!(t.temp_c.is_none());
    }
}
