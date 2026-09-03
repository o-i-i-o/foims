//! 调度器生命周期管理。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use foims_common::{AppMessage, log_debug, log_error, log_info, log_warn, msg};
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::cron::calculate_next_run;
use crate::error::{SchedulerError, SchedulerResult};
use crate::executor::TaskRegistryRef;
use crate::models::{DatabaseConfig, TaskContext};
use crate::task_log::{log_task_execution, sync_user_tasks_from_db};

/// 提取错误内部的 i18n 消息（避免拼接中文前缀导致文案泄漏到日志/数据库）
pub fn error_message(e: &SchedulerError) -> AppMessage {
    match e {
        SchedulerError::Database(m)
        | SchedulerError::NotFound(m)
        | SchedulerError::Validation(m)
        | SchedulerError::Conflict(m)
        | SchedulerError::TaskNotFound(m)
        | SchedulerError::Execution(m)
        | SchedulerError::Internal(m) => m.clone(),
    }
}

/// 调度器构建状态（添加任务后启动）
pub struct SchedulerState {
    scheduler: JobScheduler,
    registry: TaskRegistryRef,
    pool: sqlx::PgPool,
    db_config: DatabaseConfig,
}

/// 同步 job 的重叠保护释放守卫（Drop 即释放,panic 安全）
struct SyncRunningGuard(std::sync::Arc<AtomicBool>);
impl Drop for SyncRunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl SchedulerState {
    /// 创建调度器，注册任务执行器
    pub async fn new(
        pool: sqlx::PgPool,
        db_config: DatabaseConfig,
        registry: TaskRegistryRef,
    ) -> SchedulerResult<Self> {
        let scheduler = JobScheduler::new().await.map_err(|e| {
            SchedulerError::Internal(msg("server.task.scheduler.create_failed").with("error", e))
        })?;

        Ok(Self {
            scheduler,
            registry,
            pool,
            db_config,
        })
    }

    /// 添加系统定时任务，通过 registry 分发到对应执行器
    pub async fn add_system_job(
        &mut self,
        name: &str,
        cron: &str,
        task_type: &str,
        config: serde_json::Value,
    ) -> SchedulerResult<()> {
        let registry = self.registry.clone();
        let pool = self.pool.clone();
        let db_config = self.db_config.clone();
        let task_name = name.to_string();
        let task_type_owned = task_type.to_string();
        // 任务占用标志：上一轮尚未结束则跳过本轮触发，防止任务重叠执行
        let running = Arc::new(AtomicBool::new(false));

        let job = Job::new_async(cron, move |_, _| {
            let registry = registry.clone();
            let pool = pool.clone();
            let db_config = db_config.clone();
            let config = config.clone();
            let task_name = task_name.clone();
            let task_type_owned = task_type_owned.clone();
            let running = running.clone();

            Box::pin(async move {
                // 原子占位：上一轮未完成时本轮直接跳过
                if running
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    log_warn!("log.task.overlap_skipped", name = task_name);
                    return;
                }

                // 占位成功即创建 Drop 守卫：执行器 panic 或 future 被
                // 中途丢弃时由此释放标志——放在任务体之后创建则守卫
                // 尚未诞生,panic 后标志永久卡 true,任务静默停摆
                struct RunningGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);
                impl Drop for RunningGuard {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _guard = RunningGuard(running);

                // 例行成功日志的级别由执行器声明：高频维护任务降为 debug 避免刷屏
                let routine_debug = registry.debug_routine_logs(&task_type_owned);

                if routine_debug {
                    log_debug!(
                        "log.task.running",
                        name = task_name,
                        task_type = task_type_owned
                    );
                } else {
                    log_info!(
                        "log.task.running",
                        name = task_name,
                        task_type = task_type_owned
                    );
                }

                // 真实执行起点：供任务日志记录 start_time 与 duration
                let started_at = Utc::now();

                let ctx = TaskContext {
                    pool: pool.clone(),
                    config,
                    db_config,
                };

                let result = registry.execute(&task_type_owned, &ctx).await;

                match &result {
                    Ok(result_message) => {
                        if routine_debug {
                            log_debug!(
                                "log.task.completed",
                                name = task_name,
                                result = result_message
                            );
                        } else {
                            log_info!(
                                "log.task.completed",
                                name = task_name,
                                result = result_message
                            );
                        }
                        log_task_execution(
                            &pool,
                            &task_name,
                            "success",
                            result_message,
                            started_at,
                        )
                        .await;
                    }
                    Err(e) => {
                        // 失败原因以 i18n key 形式写入日志与任务日志表，由前端翻译
                        let error_text = error_message(e).log_string();
                        log_error!("log.task.failed", name = task_name, error = error_text);
                        log_task_execution(&pool, &task_name, "failed", &error_text, started_at)
                            .await;
                    }
                }
            })
        })
        .map_err(|e| {
            SchedulerError::Internal(
                msg("server.task.scheduler.job_create_failed").with("error", e),
            )
        })?;

        self.scheduler.add(job).await.map_err(|e| {
            SchedulerError::Internal(msg("server.task.scheduler.job_add_failed").with("error", e))
        })?;

        Ok(())
    }

    /// 启动调度器，内部自动注册用户任务同步 job
    pub async fn start(self) -> SchedulerResult<RunningScheduler> {
        // 添加用户任务同步 job（每5分钟；带重叠保护）
        let sync_pool = self.pool.clone();
        let sync_running = Arc::new(AtomicBool::new(false));
        let sync_job = Job::new_async("0 */5 * * * *", move |_, _| {
            let pool = sync_pool.clone();
            let running = sync_running.clone();
            Box::pin(async move {
                // 上一轮同步尚未结束则跳过本轮
                if running
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
                {
                    log_warn!("log.task.overlap_skipped", name = "user_task_sync");
                    return;
                }
                // 与 add_system_job 同款 Drop 守卫:panic 时释放占用标志
                let _guard = SyncRunningGuard(running);
                if let Err(e) = sync_user_tasks_from_db(&pool).await {
                    log_error!(
                        "log.task.sync_failed",
                        error = error_message(&e).log_string()
                    );
                }
            })
        })
        .map_err(|e| {
            SchedulerError::Internal(
                msg("server.task.scheduler.sync_job_create_failed").with("error", e),
            )
        })?;

        self.scheduler.add(sync_job).await.map_err(|e| {
            SchedulerError::Internal(
                msg("server.task.scheduler.sync_job_add_failed").with("error", e),
            )
        })?;

        // 用户任务到期派发 job（每分钟）：scheduled_tasks 中的用户任务
        // 此前只被同步 next_run_at 展示字段、从未被注册为可触发的 job，
        // 创建后静默永不自动执行。派发器以 next_run_at 为权威判定到期，
        // 执行后前移，避免动态 Job 增删的映射管理
        let dispatch_pool = self.pool.clone();
        let dispatch_registry = self.registry.clone();
        let dispatch_db_config = self.db_config.clone();
        let dispatch_running: Arc<
            std::sync::Mutex<std::collections::HashMap<uuid::Uuid, Arc<AtomicBool>>>,
        > = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let dispatch_job = Job::new_async("0 * * * * *", move |_, _| {
            let pool = dispatch_pool.clone();
            let registry = dispatch_registry.clone();
            let db_config = dispatch_db_config.clone();
            let running_map = dispatch_running.clone();
            Box::pin(async move {
                dispatch_due_user_tasks(pool, registry, db_config, running_map).await;
            })
        })
        .map_err(|e| {
            SchedulerError::Internal(
                msg("server.task.scheduler.job_create_failed").with("error", e),
            )
        })?;

        self.scheduler.add(dispatch_job).await.map_err(|e| {
            SchedulerError::Internal(msg("server.task.scheduler.job_add_failed").with("error", e))
        })?;

        self.scheduler.start().await.map_err(|e| {
            SchedulerError::Internal(msg("server.task.scheduler.start_failed").with("error", e))
        })?;

        log_info!("log.task.scheduler_started");

        Ok(RunningScheduler {
            scheduler: self.scheduler,
        })
    }
}

/// 派发到期的用户任务：逐任务查库取到期集合，带按任务 id 的重叠保护，
/// 执行后写审计日志并前移 next_run_at/last_run_at/last_result。
/// 执行前以与「立即执行」路径相同的咨询锁互斥并复核到期（due 列表是
/// 执行前的快照，慢任务期间下一轮派发可能已处理同一任务）；
/// task_logs 与状态前移在锁事务内原子提交。
async fn dispatch_due_user_tasks(
    pool: sqlx::PgPool,
    registry: TaskRegistryRef,
    db_config: DatabaseConfig,
    running_map: Arc<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, Arc<AtomicBool>>>>,
) {
    let due: Vec<crate::models::ScheduledTask> =
        match sqlx::query_as(
            "SELECT id, name, task_type, cron_expression, enabled, config, last_run_at, next_run_at, last_result, created_at, updated_at
             FROM scheduled_tasks
             WHERE enabled = true AND next_run_at IS NOT NULL AND next_run_at <= NOW()
             ORDER BY next_run_at ASC",
        )
        .fetch_all(&pool)
        .await
        {
            Ok(tasks) => tasks,
            Err(e) => {
                log_error!("log.task.query_failed", error = e);
                return;
            }
        };

    for task in due {
        // 按任务 id 的重叠保护：上一轮未结束则本轮跳过
        let flag = {
            let Ok(mut map) = running_map.lock() else {
                continue;
            };
            let flag = map
                .entry(task.id)
                .or_insert_with(|| Arc::new(AtomicBool::new(false)));
            Arc::clone(flag)
        };
        if flag
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            log_warn!("log.task.overlap_skipped", name = task.name);
            continue;
        }
        // 占位成功即创建 Drop 守卫（与系统 job 同款，panic 安全）：
        // 执行器 panic 时标志随之释放，任务不会静默停摆
        let _guard = SyncRunningGuard(flag);

        if let Err(e) = dispatch_single_task(&pool, &registry, &db_config, task).await {
            log_error!("log.task.dispatch_failed", error = e);
        }
    }
}

/// 与「立即执行」路径（src/system/scheduled_task.rs）相同键派生的任务咨询锁键，
/// 保证手动触发与到期派发对同一任务互斥
fn task_advisory_lock_key(id: uuid::Uuid) -> i64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish() as i64
}

/// 派发单个到期任务：咨询锁 → 到期复核 → 执行 → 双写（同事务原子提交）。
/// 任一步失败回滚（next_run_at 不前移，下轮派发重试）。
async fn dispatch_single_task(
    pool: &sqlx::PgPool,
    registry: &TaskRegistryRef,
    db_config: &DatabaseConfig,
    task: crate::models::ScheduledTask,
) -> Result<(), sqlx::Error> {
    let mut lock_tx = pool.begin().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock($1)")
        .bind(task_advisory_lock_key(task.id))
        .fetch_one(&mut *lock_tx)
        .await?;
    if !locked {
        // 任务正被「立即执行」或上一轮派发执行
        log_warn!("log.task.overlap_skipped", name = task.name);
        return Ok(());
    }

    // 到期复核：due 列表是派发前的快照，锁等待期间 next_run_at 可能已被
    // 并发路径前移；已处理/被停用的任务直接跳过
    let still_due: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM scheduled_tasks
          WHERE id = $1 AND enabled = true
            AND next_run_at IS NOT NULL AND next_run_at <= NOW())",
    )
    .bind(task.id)
    .fetch_one(&mut *lock_tx)
    .await?;
    if !still_due {
        return Ok(());
    }

    let started_at = Utc::now();
    let ctx = TaskContext {
        pool: pool.clone(),
        config: task.config.clone(),
        db_config: db_config.clone(),
    };
    let result = registry.execute(&task.task_type, &ctx).await;
    let end_time = Utc::now();
    let duration = i32::try_from((end_time - started_at).num_milliseconds()).unwrap_or(i32::MAX);
    let (status, details) = match &result {
        Ok(message) => (
            "success",
            serde_json::json!({ "message": message, "task_type": task.task_type }),
        ),
        Err(e) => (
            "failed",
            serde_json::json!({
                "error": error_message(e).log_string(),
                "task_type": task.task_type
            }),
        ),
    };

    // 前移 next_run_at：失败时同样前移，避免故障任务每分钟重试刷屏；
    // cron 解析失败按 1 小时退避推进（保留过期旧值会每分钟重新触发）
    let next_run = match tokio::task::spawn_blocking({
        let cron_expr = task.cron_expression.clone();
        move || calculate_next_run(&cron_expr)
    })
    .await
    {
        Ok(Ok(next)) => Some(next),
        Ok(Err(e)) => {
            log_error!(
                "log.task.next_run_update_failed",
                name = task.name,
                error = error_message(&e).log_string()
            );
            None
        }
        Err(e) => {
            log_error!("log.task.next_run_calc_task_failed", error = e);
            None
        }
    };
    let effective_next_run = next_run.unwrap_or(started_at + chrono::Duration::hours(1));

    // task_logs 与状态前移是与本次执行同源的业务双写：与锁同事务提交，
    // 任一失败整体回滚
    sqlx::query(
        r"INSERT INTO task_logs (id, task_name, status, details, start_time, end_time, duration)
           VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(&task.name)
    .bind(status)
    .bind(sqlx::types::Json(details.clone()))
    .bind(started_at)
    .bind(end_time)
    .bind(duration)
    .execute(&mut *lock_tx)
    .await?;

    sqlx::query(
        "UPDATE scheduled_tasks SET last_run_at = $1, next_run_at = $2, last_result = $3 WHERE id = $4",
    )
    .bind(started_at)
    .bind(effective_next_run)
    .bind(status)
    .bind(task.id)
    .execute(&mut *lock_tx)
    .await?;

    lock_tx.commit().await?;

    if status == "success" {
        log_info!(
            "log.task.completed",
            name = task.name,
            result = details["message"].as_str().unwrap_or_default()
        );
    } else {
        log_error!(
            "log.task.failed",
            name = task.name,
            error = details["error"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}

/// 运行中的调度器
pub struct RunningScheduler {
    scheduler: JobScheduler,
}

impl RunningScheduler {
    pub async fn shutdown(mut self) {
        if let Err(e) = self.scheduler.shutdown().await {
            log_error!("log.task.shutdown_failed", error = e);
        } else {
            log_info!("log.task.shutdown_completed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// error_message 应原样提取各变体内部消息（key 与参数都不丢失）
    #[test]
    fn error_message_提取各变体内部消息() {
        let cases: Vec<SchedulerError> = vec![
            SchedulerError::Database(msg("server.a")),
            SchedulerError::NotFound(msg("server.b")),
            SchedulerError::Validation(msg("server.c").with("field", "cron")),
            SchedulerError::Conflict(msg("server.d")),
            SchedulerError::TaskNotFound(msg("server.e").with("task_type", "backup")),
            SchedulerError::Execution(msg("server.f")),
            SchedulerError::Internal(msg("server.g")),
        ];
        let expected_keys = [
            "server.a", "server.b", "server.c", "server.d", "server.e", "server.f", "server.g",
        ];
        for (err, key) in cases.into_iter().zip(expected_keys) {
            let m = error_message(&err);
            assert_eq!(m.key(), key);
        }
    }

    #[test]
    fn error_message_保留动态参数() {
        let err = SchedulerError::Validation(
            msg("server.task.cron_expression_invalid").with("expression", "bad"),
        );
        let m = error_message(&err);
        let params = m.params();
        assert_eq!(params.len(), 1);
        assert_eq!(
            (params[0].0.as_str(), params[0].1.as_str()),
            ("expression", "bad")
        );
    }
}
