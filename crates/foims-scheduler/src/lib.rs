//! FOIMS 定时任务：基于 tokio-cron-scheduler 的调度基础设施。

pub mod cron;
pub mod error;
pub mod executor;
pub mod models;
pub mod scheduler;
pub mod task_log;

pub use cron::calculate_next_run;
pub use error::{SchedulerError, SchedulerResult};
pub use executor::{TaskExecutor, TaskRegistry, TaskRegistryRef};
pub use models::{DatabaseConfig, ScheduledTask, TaskContext, TaskLog};
pub use scheduler::{RunningScheduler, SchedulerState, error_message};
pub use task_log::{log_task_execution, sync_user_tasks_from_db};
