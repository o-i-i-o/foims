use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};

use crate::error::{SchedulerError, SchedulerResult};
use crate::executor::TaskRegistryRef;
use crate::models::{DatabaseConfig, TaskContext};
use crate::task_log::{log_task_execution, sync_user_tasks_from_db};

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
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| SchedulerError::Internal(format!("创建调度器失败: {e}")))?;

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
                info!(
                    "Running scheduled task: {} ({})...",
                    task_name, task_type_owned
                );

                let ctx = TaskContext {
                    pool: pool.clone(),
                    config,
                    db_config,
                };

                let result = registry.execute(&task_type_owned, &ctx).await;

                match &result {
                    Ok(msg) => {
                        info!("Task {} completed: {}", task_name, msg);
                        log_task_execution(&pool, &task_name, "success", msg).await;
                    }
                    Err(e) => {
                        error!("Task {} failed: {}", task_name, e);
                        log_task_execution(&pool, &task_name, "failed", &e.to_string()).await;
                    }
                }
            })
        })
        .map_err(|e| SchedulerError::Internal(format!("创建定时任务失败: {e}")))?;

        self.scheduler
            .add(job)
            .await
            .map_err(|e| SchedulerError::Internal(format!("添加定时任务失败: {e}")))?;

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
                    error!("Failed to sync user tasks: {}", e);
                }
            })
        })
        .map_err(|e| SchedulerError::Internal(format!("创建任务同步job失败: {e}")))?;

        self.scheduler
            .add(sync_job)
            .await
            .map_err(|e| SchedulerError::Internal(format!("添加任务同步job失败: {e}")))?;

        self.scheduler
            .start()
            .await
            .map_err(|e| SchedulerError::Internal(format!("启动调度器失败: {e}")))?;

        info!("Cron scheduler started successfully");

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
            error!("关闭调度器失败: {}", e);
        } else {
            info!("调度器已关闭");
        }
    }
}
