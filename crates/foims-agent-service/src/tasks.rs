//! Agent 监控闭环调度任务执行器（设计 docs/agent-design.md §5）。
//!
//! 由主程序 TaskRegistry 注册、系统级定时任务驱动：
//! - `agent_offline`：每分钟按 上报间隔 × 离线倍数 将失联 active agent 置为 offline；
//! - `agent_history_cleanup`：每日清理超过保留期的 agent_metrics_history；
//! - `agent_snmp_poll`：周期轮询已配置 SNMP 的设备，采集结果以 source='snmp'
//!   的 agents 行呈现于主机监控页（详见 [`crate::snmp_poll`]）。

use async_trait::async_trait;
use foims_scheduler::{SchedulerError, SchedulerResult, TaskContext, TaskExecutor};
use serde_json::Value;

use foims_common::{log_debug, msg};

use crate::snmp_poll;

/// 离线判定参数缺省值（与 AgentConfig 默认一致）
const DEFAULT_INTERVAL_SECS: u64 = 60;
const DEFAULT_OFFLINE_FACTOR: u64 = 3;
/// SNMP 轮询间隔缺省值（秒，调度注册侧保持一致）
const DEFAULT_SNMP_POLL_INTERVAL_SECS: u64 = 300;
/// 历史保留天数缺省值与上限（对齐天数类任务的既有口径）
const DEFAULT_RETENTION_DAYS: i64 = 30;
const MAX_RETENTION_DAYS: i64 = 3650;

/// 从任务配置读取 u64 参数（缺失/类型不符回落默认值）。
fn config_u64(config: &Value, key: &str, default: u64) -> u64 {
    config
        .get(key)
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// Agent 离线判定任务执行器：last_seen 超过 上报间隔 × 倍数 的
/// active agent 置为 offline（pending/disabled/revoked 不参与判定）。
/// 仅对 source='agent' 生效：SNMP 采集行轮询间隔更长（默认 5 分钟），
/// 由 snmp_poll 按自身间隔 × 2 独立判定离线。
pub struct AgentOfflineTaskExecutor;

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
        let interval_secs = config_u64(&ctx.config, "interval_secs", DEFAULT_INTERVAL_SECS);
        let factor = config_u64(&ctx.config, "factor", DEFAULT_OFFLINE_FACTOR);
        let seconds = interval_secs.saturating_mul(factor);

        let result = sqlx::query(
            r"UPDATE agents SET status = 'offline'
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
/// agent_metrics_history 行（曲线快照体积随时间增长，需滚动清理）。
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
            r"DELETE FROM agent_metrics_history
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
        Ok("server.task.agent_history_cleanup_completed".to_string())
    }
}

/// SNMP 设备轮询任务执行器：采集已配置 SNMP 的设备（MIB-II 系统组），
/// 结果以 source='snmp' 的 agents 行呈现于主机监控页，并按
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
