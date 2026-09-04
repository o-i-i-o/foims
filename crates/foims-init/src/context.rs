//! 初始化上下文（连接信息与请求参数封装）。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::types::DatabaseConfig;

/// 初始化完成后的重启模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartMode {
    /// 已注册 systemd 单元：重启命令已下发（systemctl restart）。
    Systemd,
    /// 未注册 systemd 单元（程序并非以服务运行）：不执行重启，
    /// 由用户手动重启程序完成初始化。
    Manual,
}

pub type RestartFuture = Pin<Box<dyn Future<Output = Result<RestartMode, String>> + Send>>;

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
    /// 一次性重启许可：仅 init 完成时武装，重启端点消费后立即解除。
    /// 否则「init 关闭后拒绝重启」的守卫会连带拒绝刚完成初始化的合法重启，
    /// 而完全放开又会让重启端点在初始化前/后被滥用为任意重启入口。
    restart_armed: Arc<AtomicBool>,
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
            restart_armed: Arc::new(AtomicBool::new(false)),
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

    /// 初始化完成时武装一次性重启许可（由 init_system 在落盘后调用）
    pub fn arm_restart(&self) {
        self.restart_armed.store(true, Ordering::SeqCst);
    }

    /// 消费一次性重启许可：武装状态下返回 true 并立即解除武装
    pub fn consume_restart_arm(&self) -> bool {
        self.restart_armed.swap(false, Ordering::SeqCst)
    }
}
