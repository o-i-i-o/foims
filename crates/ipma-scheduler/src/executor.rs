//! 定时任务执行器。

use crate::error::{SchedulerError, SchedulerResult};
use crate::models::TaskContext;
use async_trait::async_trait;
use ipma_common::msg;
use std::collections::HashMap;
use std::sync::Arc;

/// 任务执行器 trait，由各业务模块实现
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    fn task_type(&self) -> &str;
    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String>;

    /// 例行成功日志（任务开始/完成）是否降为 debug 级别。
    /// 高频维护型任务应返回 true，避免例行成功日志刷屏；失败日志仍为 error，不受影响。
    fn debug_routine_logs(&self) -> bool {
        false
    }
}

/// 任务注册表，按 task_type 分发到对应的执行器
pub struct TaskRegistry {
    executors: HashMap<String, Box<dyn TaskExecutor>>,
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self {
            executors: HashMap::new(),
        }
    }

    pub fn register(&mut self, executor: Box<dyn TaskExecutor>) {
        self.executors
            .insert(executor.task_type().to_string(), executor);
    }

    pub async fn execute(&self, task_type: &str, ctx: &TaskContext) -> SchedulerResult<String> {
        match self.executors.get(task_type) {
            Some(executor) => executor.execute(ctx).await,
            None => Err(SchedulerError::TaskNotFound(
                msg("server.task.type_unknown").with("task_type", task_type),
            )),
        }
    }

    /// 查询指定任务类型的例行成功日志是否降为 debug（未注册类型按 info 处理）
    pub fn debug_routine_logs(&self, task_type: &str) -> bool {
        self.executors
            .get(task_type)
            .is_some_and(|executor| executor.debug_routine_logs())
    }
}

/// 异步安全的任务注册表引用
pub type TaskRegistryRef = Arc<TaskRegistry>;
