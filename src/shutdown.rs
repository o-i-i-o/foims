//! 优雅退出处理。

use ipma_common::{log_error, log_info, log_warn};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

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
            log_info!("log.shutdown.requested");
            drop(self.sender.send(()));
        }
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
            log_error!("log.shutdown.sigint_register_failed", error = e);
            return;
        }
    };

    let mut sigterm = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        Ok(s) => s,
        Err(e) => {
            log_error!("log.shutdown.sigterm_register_failed", error = e);
            return;
        }
    };

    let mut sighup = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) {
        Ok(s) => s,
        Err(e) => {
            log_error!("log.shutdown.sighup_register_failed", error = e);
            return;
        }
    };

    // 订阅进程内关闭广播：register_service（服务让位）等业务路径触发的
    // request_shutdown 与 OS 信号同等待遇，唤醒优雅退出流程
    let mut internal = shutdown.subscribe();

    loop {
        tokio::select! {
            _ = sigint.recv() => {
                log_info!("log.shutdown.sigint_received");
                shutdown.request_shutdown();
                break;
            }
            _ = sigterm.recv() => {
                log_info!("log.shutdown.sigterm_received");
                shutdown.request_shutdown();
                break;
            }
            _ = sighup.recv() => {
                log_warn!("log.shutdown.sighup_ignored");
                continue;
            }
            _ = internal.recv() => {
                break;
            }
        }
    }
}
