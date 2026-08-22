//! 调度器生命周期管理。

use ipma_common::{AppMessage, log_debug, log_error, log_info, msg};
use tokio_cron_scheduler::{Job, JobScheduler};

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

        let job = Job::new_async(cron, move |_, _| {
            let registry = registry.clone();
            let pool = pool.clone();
            let db_config = db_config.clone();
            let config = config.clone();
            let task_name = task_name.clone();
            let task_type_owned = task_type_owned.clone();

            Box::pin(async move {
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
                        log_task_execution(&pool, &task_name, "success", result_message).await;
                    }
                    Err(e) => {
                        // 失败原因以 i18n key 形式写入日志与任务日志表，由前端翻译
                        let error_text = error_message(e).log_string();
                        log_error!("log.task.failed", name = task_name, error = error_text);
                        log_task_execution(&pool, &task_name, "failed", &error_text).await;
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
        // 添加用户任务同步 job（每5分钟）
        let sync_pool = self.pool.clone();
        let sync_job = Job::new_async("0 */5 * * * *", move |_, _| {
            let pool = sync_pool.clone();
            Box::pin(async move {
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

        self.scheduler.start().await.map_err(|e| {
            SchedulerError::Internal(msg("server.task.scheduler.start_failed").with("error", e))
        })?;

        log_info!("log.task.scheduler_started");

        Ok(RunningScheduler {
            scheduler: self.scheduler,
        })
    }
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
