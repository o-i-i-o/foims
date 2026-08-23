//! 初始化上下文（连接信息与请求参数封装）。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::types::DatabaseConfig;

pub type RestartFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

#[derive(Clone)]
pub struct InitContext {
    pub db_config: DatabaseConfig,
    pub config_path: String,
    /// 初始化模式开关（内存态）。
    ///
    /// init 完成后立即翻转为 false（I-3）：若仅在配置文件中落盘，
    /// 从「完成初始化」到「服务重启」之间存在毁库窗口——持有验证码者
    /// 仍可调用清库/重建接口。
    init_enabled: Arc<AtomicBool>,
    pub restart_fn: Arc<dyn Fn() -> RestartFuture + Send + Sync>,
}

impl InitContext {
    pub fn new(
        db_config: DatabaseConfig,
        config_path: String,
        init_enabled: bool,
        restart_fn: Arc<dyn Fn() -> RestartFuture + Send + Sync>,
    ) -> Self {
        Self {
            db_config,
            config_path,
            init_enabled: Arc::new(AtomicBool::new(init_enabled)),
            restart_fn,
        }
    }

    /// 当前初始化模式是否开启（内存实时值）
    pub fn init_enabled(&self) -> bool {
        self.init_enabled.load(Ordering::SeqCst)
    }

    /// 立即关闭初始化模式（内存生效，配置文件持久化由调用方负责）
    pub fn disable_init(&self) {
        self.init_enabled.store(false, Ordering::SeqCst);
    }
}
