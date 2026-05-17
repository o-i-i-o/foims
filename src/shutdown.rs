use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

#[derive(Clone)]
pub struct ShutdownSignal {
    sender: broadcast::Sender<()>,
    shutdown_requested: Arc<AtomicBool>,
}

impl ShutdownSignal {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1);
        Self {
            sender,
            shutdown_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }

    pub fn request_shutdown(&self) {
        if self
            .shutdown_requested
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            info!("收到关闭信号，开始优雅关闭...");
            drop(self.sender.send(()));
        }
    }

    #[must_use]
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::Relaxed)
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn wait_for_shutdown_signal(shutdown: &ShutdownSignal) {
    let mut sigint = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
    {
        Ok(s) => s,
        Err(e) => {
            error!("无法注册 SIGINT 信号处理器: {}", e);
            return;
        }
    };

    let mut sigterm = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        Ok(s) => s,
        Err(e) => {
            error!("无法注册 SIGTERM 信号处理器: {}", e);
            return;
        }
    };

    let mut sighup = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) {
        Ok(s) => s,
        Err(e) => {
            error!("无法注册 SIGHUP 信号处理器: {}", e);
            return;
        }
    };

    loop {
        tokio::select! {
            _ = sigint.recv() => {
                info!("收到 SIGINT 信号");
                shutdown.request_shutdown();
                break;
            }
            _ = sigterm.recv() => {
                info!("收到 SIGTERM 信号");
                shutdown.request_shutdown();
                break;
            }
            _ = sighup.recv() => {
                warn!("收到 SIGHUP 信号，忽略 (如需重载配置请使用 API)");
                continue;
            }
        }
    }
}
