//! Agent 监控闭环调度任务执行器（设计 docs/agent-design.md §5）：由主程序
//! TaskRegistry 注册、系统级定时任务驱动的 agent 离线判定、历史指标清理、
//! SNMP 与 IPMI 设备轮询四个执行器（采集细节见 [`crate::snmp_poll`] 与
//! [`crate::ipmi_poll`]）。

use async_trait::async_trait;
use foims_scheduler::{SchedulerError, SchedulerResult, TaskContext, TaskExecutor};
use serde_json::Value;

use foims_common::{log_debug, msg};

use crate::ipmi_poll;
use crate::snmp_poll;

/// SNMP 轮询间隔缺省值（秒，调度注册侧保持一致）
const DEFAULT_SNMP_POLL_INTERVAL_SECS: u64 = 300;
/// IPMI 轮询间隔缺省值（秒，与 SNMP 同 5 分钟节奏）
const DEFAULT_IPMI_POLL_INTERVAL_SECS: u64 = 300;
/// 历史保留天数缺省值与上限（天数类清理任务共用口径）
const DEFAULT_RETENTION_DAYS: i64 = 30;
const MAX_RETENTION_DAYS: i64 = 3650;

/// 从任务配置读取正整数参数（缺失、类型不符或 ≤0 时回落默认值）。
fn config_u64(config: &Value, key: &str, default: u64) -> u64 {
    config
        .get(key)
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// Agent 离线判定任务执行器：last_seen 超过 上报间隔 × 倍数 的
/// active agent 置为 offline（disabled 不参与判定）。
/// 仅对 source='agent' 生效：SNMP 采集行轮询间隔更长（默认 5 分钟），
/// 由 snmp_poll 按自身间隔 × 2 独立判定离线。
///
/// 判定参数与上报间隔同源（AgentConfig）：默认值在注册时由主程序从
/// agent 配置段注入，避免「上报间隔已改而离线阈值仍用旧默认」的错判；
/// 任务配置显式给出 interval_secs/factor 时仍以任务配置优先。
pub struct AgentOfflineTaskExecutor {
    /// 默认判定间隔（秒），注册时取自 AgentConfig.report_interval_secs
    default_interval_secs: u64,
    /// 默认离线倍数，注册时取自 AgentConfig.offline_factor
    default_factor: u64,
}

impl AgentOfflineTaskExecutor {
    pub fn new(default_interval_secs: u64, default_factor: u64) -> Self {
        Self {
            default_interval_secs,
            default_factor,
        }
    }
}

#[async_trait]
impl TaskExecutor for AgentOfflineTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_offline"
    }

    /// 每分钟例行运行且多数轮次无翻转，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let interval_secs = config_u64(&ctx.config, "interval_secs", self.default_interval_secs);
        let factor = config_u64(&ctx.config, "factor", self.default_factor);
        let seconds = interval_secs.saturating_mul(factor);

        let result = sqlx::query(
            r"UPDATE monitored_hosts SET status = 'offline'
               WHERE status = 'active'
                 AND source = 'agent'
                 AND last_seen IS NOT NULL
                 AND last_seen < NOW() - make_interval(secs => $1)",
        )
        .bind(seconds as f64)
        .execute(&ctx.pool)
        .await
        .map_err(|e| {
            SchedulerError::Execution(msg("server.task.agent_offline_failed").with("error", e))
        })?;

        log_debug!(
            "log.task.agent_offline_completed",
            count = result.rows_affected()
        );
        Ok("server.task.agent_offline_completed".to_string())
    }
}

/// Agent 历史指标清理任务执行器：删除 collected_at 超过保留期的
/// host_metrics_history 行（曲线快照体积随时间增长，需滚动清理）。
pub struct AgentHistoryCleanupTaskExecutor;

#[async_trait]
impl TaskExecutor for AgentHistoryCleanupTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_history_cleanup"
    }

    /// 每日例行运行，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let days = ctx
            .config
            .get("retention_days")
            .and_then(Value::as_i64)
            .filter(|d| (1..=MAX_RETENTION_DAYS).contains(d))
            .unwrap_or(DEFAULT_RETENTION_DAYS);

        let result = sqlx::query(
            r"DELETE FROM host_metrics_history
               WHERE collected_at < NOW() - make_interval(days => $1)",
        )
        .bind(i32::try_from(days).unwrap_or(i32::MAX))
        .execute(&ctx.pool)
        .await
        .map_err(|e| {
            SchedulerError::Execution(
                msg("server.task.agent_history_cleanup_failed").with("error", e),
            )
        })?;

        log_debug!(
            "log.task.agent_history_cleanup_completed",
            count = result.rows_affected()
        );

        // 同任务内追加清理 snmp_events：保留天数与 host_metrics_history 同源，
        // 均取任务 config（注册侧由 agent 配置段注入，缺省 30，范围 1-3650）
        let snmp_days = ctx
            .config
            .get("snmp_event_retention_days")
            .and_then(Value::as_i64)
            .filter(|d| (1..=MAX_RETENTION_DAYS).contains(d))
            .unwrap_or(DEFAULT_RETENTION_DAYS);
        let snmp_result = sqlx::query(
            r"DELETE FROM snmp_events
               WHERE occurred_at < NOW() - make_interval(days => $1)",
        )
        .bind(i32::try_from(snmp_days).unwrap_or(i32::MAX))
        .execute(&ctx.pool)
        .await
        .map_err(|e| {
            SchedulerError::Execution(
                msg("server.task.agent_history_cleanup_failed").with("error", e),
            )
        })?;
        log_debug!(
            "log.snmp.events_cleanup_completed",
            count = snmp_result.rows_affected()
        );

        Ok("server.task.agent_history_cleanup_completed".to_string())
    }
}

/// SNMP 设备轮询任务执行器：采集已配置 SNMP 的设备（MIB-II 系统组），
/// 结果以 source='snmp' 的 monitored_hosts 行呈现于主机监控页，并按
/// 轮询间隔 × 2 翻转连续失联设备为 offline。
pub struct AgentSnmpPollTaskExecutor;

#[async_trait]
impl TaskExecutor for AgentSnmpPollTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_snmp_poll"
    }

    /// 5 分钟例行运行，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let interval_secs = config_u64(
            &ctx.config,
            "interval_secs",
            DEFAULT_SNMP_POLL_INTERVAL_SECS,
        );
        let (ok, fail) = snmp_poll::poll_all(&ctx.pool, interval_secs).await;
        log_debug!("log.task.agent_snmp_poll_completed", ok = ok, fail = fail);
        Ok("server.task.agent_snmp_poll_completed".to_string())
    }
}

/// IPMI 设备带外轮询任务执行器：采集已配置 IPMI 凭据的设备（Get Device ID +
/// Chassis Status + SDR 传感器），结果以 source='ipmi' 的 monitored_hosts 行
/// 呈现于主机监控页，并按轮询间隔 × 2 翻转连续失联设备为 offline
/// （采集细节见 [`crate::ipmi_poll`]）。
pub struct AgentIpmiPollTaskExecutor;

#[async_trait]
impl TaskExecutor for AgentIpmiPollTaskExecutor {
    fn task_type(&self) -> &str {
        "agent_ipmi_poll"
    }

    /// 5 分钟例行运行，成功日志降为 debug 避免刷屏
    fn debug_routine_logs(&self) -> bool {
        true
    }

    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
        let interval_secs = config_u64(
            &ctx.config,
            "interval_secs",
            DEFAULT_IPMI_POLL_INTERVAL_SECS,
        );
        let (ok, fail) = ipmi_poll::poll_all(&ctx.pool, interval_secs).await;
        log_debug!("log.task.agent_ipmi_poll_completed", ok = ok, fail = fail);
        Ok("server.task.agent_ipmi_poll_completed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 读取正整数否则回落默认() {
        let cfg = serde_json::json!({"interval_secs": 60, "zero": 0, "neg": -1, "text": "x"});
        assert_eq!(config_u64(&cfg, "interval_secs", 300), 60);
        // 缺失键 → 默认
        assert_eq!(config_u64(&cfg, "missing", 300), 300);
        // 0 被过滤 → 默认
        assert_eq!(config_u64(&cfg, "zero", 300), 300);
        // 负数/非数字（as_u64 返回 None）→ 默认
        assert_eq!(config_u64(&cfg, "neg", 300), 300);
        assert_eq!(config_u64(&cfg, "text", 300), 300);
    }
}
