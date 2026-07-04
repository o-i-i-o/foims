use crate::error::{SchedulerError, SchedulerResult};
use crate::models::TaskContext;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// 任务执行器 trait，由各业务模块实现
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    fn task_type(&self) -> &str;
    async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String>;
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
            None => Err(SchedulerError::TaskNotFound(format!(
                "未知的任务类型: {task_type}"
            ))),
        }
    }

    pub fn task_types(&self) -> Vec<&str> {
        self.executors.keys().map(|s| s.as_str()).collect()
    }
}

/// 异步安全的任务注册表引用
pub type TaskRegistryRef = Arc<TaskRegistry>;
