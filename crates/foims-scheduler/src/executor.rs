//! 定时任务执行器。

use crate::error::{SchedulerError, SchedulerResult};
use crate::models::TaskContext;
use async_trait::async_trait;
use foims_common::msg;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DatabaseConfig;

    /// 回显执行器：返回携带上下文配置的结果 key
    struct EchoExecutor {
        task_type: &'static str,
        debug_logs: bool,
    }

    #[async_trait]
    impl TaskExecutor for EchoExecutor {
        fn task_type(&self) -> &str {
            self.task_type
        }

        async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
            Ok(format!("server.task.echo:{}", ctx.config))
        }

        fn debug_routine_logs(&self) -> bool {
            self.debug_logs
        }
    }

    /// 构造测试上下文：connect_lazy 不建立真实连接，仅解析 URL
    fn make_context() -> TaskContext {
        let pool = sqlx::pool::PoolOptions::<sqlx::Postgres>::new()
            .connect_lazy("postgres://127.0.0.1:5432/foims_test")
            .unwrap_or_else(|e| panic!("构造惰性连接池失败: {e}"));
        TaskContext {
            pool,
            config: serde_json::json!({ "echo": true }),
            db_config: DatabaseConfig {
                host: "127.0.0.1".to_string(),
                port: 5432,
                database: "foims_test".to_string(),
                username: "u".to_string(),
                password: "p".to_string(),
                max_connections: 10,
                min_connections: 5,
                acquire_timeout_secs: 15,
                idle_timeout_secs: 60,
                max_lifetime_secs: 1800,
                query_timeout_secs: 30,
                health_check_interval_secs: 30,
            },
        }
    }

    #[tokio::test]
    async fn 注册后按类型分发执行() {
        let mut registry = TaskRegistry::new();
        registry.register(Box::new(EchoExecutor {
            task_type: "echo",
            debug_logs: false,
        }));
        let ctx = make_context();
        let Ok(result) = registry.execute("echo", &ctx).await else {
            panic!("已注册任务应执行成功");
        };
        assert!(
            result.starts_with("server.task.echo:"),
            "执行结果: {result}"
        );
    }

    #[tokio::test]
    async fn 未注册类型返回任务未找到() {
        let registry = TaskRegistry::new();
        let ctx = make_context();
        let Err(SchedulerError::TaskNotFound(m)) = registry.execute("missing", &ctx).await else {
            panic!("未注册类型应返回 TaskNotFound");
        };
        assert_eq!(m.key(), "server.task.type_unknown");
        let params = m.params();
        assert_eq!(
            (params[0].0.as_str(), params[0].1.as_str()),
            ("task_type", "missing")
        );
    }

    #[tokio::test]
    async fn 同类型重复注册以后者为准() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static CALLS: AtomicUsize = AtomicUsize::new(0);

        struct CountingExecutor {
            tag: &'static str,
        }

        #[async_trait]
        impl TaskExecutor for CountingExecutor {
            fn task_type(&self) -> &str {
                "count"
            }

            async fn execute(&self, ctx: &TaskContext) -> SchedulerResult<String> {
                CALLS.fetch_add(1, Ordering::SeqCst);
                Ok(format!("{}:{}", self.tag, ctx.config))
            }
        }

        let mut registry = TaskRegistry::new();
        registry.register(Box::new(CountingExecutor { tag: "first" }));
        registry.register(Box::new(CountingExecutor { tag: "second" }));
        let ctx = make_context();
        let result = registry
            .execute("count", &ctx)
            .await
            .unwrap_or_else(|e| panic!("执行失败: {e}"));
        assert!(
            result.starts_with("second:"),
            "后注册者应覆盖前者: {result}"
        );
        assert_eq!(CALLS.fetch_add(0, Ordering::SeqCst), 1);
    }

    #[test]
    fn 例行日志级别声明_按执行器与注册状态判定() {
        let mut registry = TaskRegistry::new();
        registry.register(Box::new(EchoExecutor {
            task_type: "quiet",
            debug_logs: true,
        }));
        registry.register(Box::new(EchoExecutor {
            task_type: "loud",
            debug_logs: false,
        }));
        assert!(registry.debug_routine_logs("quiet"));
        assert!(!registry.debug_routine_logs("loud"));
        assert!(
            !registry.debug_routine_logs("missing"),
            "未注册类型按 info 处理"
        );
    }
}
