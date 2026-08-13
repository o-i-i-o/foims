use arc_swap::ArcSwap;
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info, warn};

use crate::config::DatabaseConfig;

pub fn url_encode_component(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    result
}

#[derive(Debug, Default)]
pub struct PoolMetrics {
    pub active_connections: AtomicU32,
    pub idle_connections: AtomicU32,
    pub waiting_requests: AtomicU32,
    pub total_requests: AtomicU64,
    pub failed_requests: AtomicU64,
    pub avg_wait_time_ms: AtomicU64,
    pub last_updated: AtomicU64,
    pub leak_warning_count: AtomicU32,
}

impl PoolMetrics {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            active_connections: AtomicU32::new(0),
            idle_connections: AtomicU32::new(0),
            waiting_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            avg_wait_time_ms: AtomicU64::new(0),
            last_updated: AtomicU64::new(0),
            leak_warning_count: AtomicU32::new(0),
        }
    }

    pub fn record_request_start(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.waiting_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_request_complete(&self, wait_time_ms: u64, success: bool) {
        self.waiting_requests.fetch_sub(1, Ordering::Relaxed);

        if !success {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }

        self.update_avg_wait_time(wait_time_ms);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_else(|e| {
                tracing::warn!("系统时间异常: {}, 使用0作为时间戳", e);
                0
            });
        self.last_updated.store(now, Ordering::Relaxed);
    }

    fn update_avg_wait_time(&self, wait_time_ms: u64) {
        let mut current = self.avg_wait_time_ms.load(Ordering::Relaxed);
        loop {
            let new_avg = if current == 0 {
                wait_time_ms
            } else {
                (current * 9 + wait_time_ms) / 10
            };
            match self.avg_wait_time_ms.compare_exchange_weak(
                current,
                new_avg,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    pub fn update_connection_counts(&self, active: u32, idle: u32) {
        self.active_connections.store(active, Ordering::Relaxed);
        self.idle_connections.store(idle, Ordering::Relaxed);
    }

    pub fn record_leak_warning(&self) {
        self.leak_warning_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> PoolMetricsSnapshot {
        PoolMetricsSnapshot {
            active_connections: self.active_connections.load(Ordering::Relaxed),
            idle_connections: self.idle_connections.load(Ordering::Relaxed),
            waiting_requests: self.waiting_requests.load(Ordering::Relaxed),
            total_requests: self.total_requests.load(Ordering::Relaxed),
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            avg_wait_time_ms: self.avg_wait_time_ms.load(Ordering::Relaxed),
            last_updated: self.last_updated.load(Ordering::Relaxed),
            leak_warning_count: self.leak_warning_count.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolMetricsSnapshot {
    pub active_connections: u32,
    pub idle_connections: u32,
    pub waiting_requests: u32,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub avg_wait_time_ms: u64,
    pub last_updated: u64,
    pub leak_warning_count: u32,
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
    pub idle_timeout_secs: u64,
    pub max_lifetime_secs: u64,
    pub test_before_acquire: bool,
    pub health_check_interval_secs: u64,
    pub auto_scaling_enabled: bool,
    pub low_load_threshold: f32,
    pub high_load_threshold: f32,
    pub scaling_cooldown_secs: u64,
    pub query_timeout_secs: u64,
    pub slow_query_threshold_ms: u64,
    pub retry_max_attempts: u32,
    pub retry_base_delay_ms: u64,
    pub leak_detection_threshold: f32,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 5,
            acquire_timeout_secs: 15,
            idle_timeout_secs: 60,
            max_lifetime_secs: 1800,
            test_before_acquire: true,
            health_check_interval_secs: 30,
            auto_scaling_enabled: false,
            low_load_threshold: 0.3,
            high_load_threshold: 0.8,
            scaling_cooldown_secs: 60,
            query_timeout_secs: 30,
            slow_query_threshold_ms: 1000,
            retry_max_attempts: 3,
            retry_base_delay_ms: 100,
            leak_detection_threshold: 0.9,
        }
    }
}

impl PoolConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections == 0 {
            return Err("max_connections must be greater than 0".to_string());
        }
        if self.min_connections > self.max_connections {
            return Err(format!(
                "min_connections ({}) must be less than or equal to max_connections ({})",
                self.min_connections, self.max_connections
            ));
        }
        if self.acquire_timeout_secs == 0 {
            return Err("acquire_timeout_secs must be greater than 0".to_string());
        }
        if self.idle_timeout_secs == 0 {
            return Err("idle_timeout_secs must be greater than 0".to_string());
        }
        if self.max_lifetime_secs == 0 {
            return Err("max_lifetime_secs must be greater than 0".to_string());
        }
        if self.max_lifetime_secs < self.idle_timeout_secs {
            return Err(format!(
                "max_lifetime_secs ({}) should be greater than or equal to idle_timeout_secs ({})",
                self.max_lifetime_secs, self.idle_timeout_secs
            ));
        }
        if self.query_timeout_secs == 0 {
            return Err("query_timeout_secs must be greater than 0".to_string());
        }
        if self.retry_max_attempts == 0 {
            return Err("retry_max_attempts must be greater than 0".to_string());
        }
        if self.low_load_threshold >= self.high_load_threshold {
            return Err(format!(
                "low_load_threshold ({}) must be less than high_load_threshold ({})",
                self.low_load_threshold, self.high_load_threshold
            ));
        }
        if self.leak_detection_threshold <= 0.0 || self.leak_detection_threshold > 1.0 {
            return Err(format!(
                "leak_detection_threshold ({}) must be in range (0.0, 1.0]",
                self.leak_detection_threshold
            ));
        }
        Ok(())
    }
}

impl From<&DatabaseConfig> for PoolConfig {
    fn from(config: &DatabaseConfig) -> Self {
        Self {
            max_connections: config.max_connections,
            min_connections: config.min_connections.min(config.max_connections),
            acquire_timeout_secs: config.acquire_timeout_secs,
            idle_timeout_secs: config.idle_timeout_secs,
            max_lifetime_secs: config.max_lifetime_secs,
            test_before_acquire: true,
            health_check_interval_secs: config.health_check_interval_secs,
            auto_scaling_enabled: false,
            low_load_threshold: 0.3,
            high_load_threshold: 0.8,
            scaling_cooldown_secs: 60,
            query_timeout_secs: config.query_timeout_secs,
            slow_query_threshold_ms: config.slow_query_threshold_ms,
            retry_max_attempts: 3,
            retry_base_delay_ms: 100,
            leak_detection_threshold: 0.9,
        }
    }
}

#[derive(Clone)]
pub struct DbPool {
    pool: Arc<ArcSwap<PgPool>>,
    pub metrics: Arc<PoolMetrics>,
    pub config: Arc<RwLock<PoolConfig>>,
    pub db_config: DatabaseConfig,
}

impl DbPool {
    pub async fn new(config: &DatabaseConfig) -> Result<Self, sqlx::Error> {
        let pool_config = PoolConfig::from(config);
        Self::new_with_config(config, pool_config).await
    }

    pub async fn new_with_config(
        config: &DatabaseConfig,
        pool_config: PoolConfig,
    ) -> Result<Self, sqlx::Error> {
        if let Err(e) = pool_config.validate() {
            return Err(sqlx::Error::Configuration(e.into()));
        }

        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            url_encode_component(&config.username),
            url_encode_component(&config.password),
            config.host,
            config.port,
            url_encode_component(&config.database)
        );

        let pool = Self::create_pool(&url, &pool_config).await?;

        info!(
            "数据库连接池创建成功: max={}, min={}, acquire_timeout={}s, idle_timeout={}s, max_lifetime={}s",
            pool_config.max_connections,
            pool_config.min_connections,
            pool_config.acquire_timeout_secs,
            pool_config.idle_timeout_secs,
            pool_config.max_lifetime_secs
        );

        Ok(Self {
            pool: Arc::new(ArcSwap::from(Arc::new(pool))),
            metrics: Arc::new(PoolMetrics::new()),
            config: Arc::new(RwLock::new(pool_config)),
            db_config: config.clone(),
        })
    }

    fn get_pool(&self) -> Arc<PgPool> {
        self.pool.load_full()
    }

    async fn create_pool(url: &str, config: &PoolConfig) -> Result<PgPool, sqlx::Error> {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
            .idle_timeout(Some(Duration::from_secs(config.idle_timeout_secs)))
            .max_lifetime(Some(Duration::from_secs(config.max_lifetime_secs)))
            .test_before_acquire(config.test_before_acquire)
            .connect(url)
            .await
    }

    #[must_use]
    pub fn get_conn(&self) -> PgPool {
        self.update_metrics();
        (*self.get_pool()).clone()
    }

    pub async fn begin(&self) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
        let pool = self.get_pool();
        pool.begin().await
    }

    pub async fn health_check(&self) -> Result<bool, sqlx::Error> {
        let pool = self.get_pool();
        let result = time::timeout(
            Duration::from_secs(5),
            sqlx::query("SELECT 1").execute(&*pool),
        )
        .await;
        match result {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(sqlx::Error::PoolTimedOut),
        }
    }

    fn update_metrics(&self) {
        let pool = self.get_pool();
        let total = pool.size();
        let idle = pool.num_idle() as u32;
        let active = total.saturating_sub(idle);
        self.metrics.update_connection_counts(active, idle);
    }

    fn check_connection_leak(&self) {
        let pool = self.get_pool();
        let total = pool.size();
        let idle = pool.num_idle() as u32;
        let active = total.saturating_sub(idle);

        if total == 0 {
            return;
        }

        let utilization = active as f32 / total as f32;
        let config = self.config.try_read();
        let threshold = config
            .as_ref()
            .map(|c| c.leak_detection_threshold)
            .unwrap_or(0.9);

        if utilization >= threshold {
            self.metrics.record_leak_warning();
            warn!(
                "连接池接近耗尽，可能存在连接泄漏! 活跃: {}/{}, 利用率: {:.1}%, 阈值: {:.1}%",
                active,
                total,
                utilization * 100.0,
                threshold * 100.0
            );
        }

        if active == total && total > 0 {
            error!(
                "连接池已完全耗尽! 所有 {} 个连接都在使用中，等待队列: {}",
                total,
                self.metrics.waiting_requests.load(Ordering::Relaxed)
            );
        }
    }

    #[must_use]
    pub fn get_metrics(&self) -> PoolMetricsSnapshot {
        self.metrics.snapshot()
    }

    #[must_use]
    pub fn get_pool_status(&self) -> PoolStatus {
        let pool = self.get_pool();
        PoolStatus {
            size: pool.size(),
            num_idle: pool.num_idle() as u32,
            is_closed: pool.is_closed(),
        }
    }

    pub fn start_health_check_task(
        &self,
        interval_seconds: u64,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) {
        let pool_clone = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(interval_seconds));
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let check_result = pool_clone.health_check().await;
                        match check_result {
                            Ok(_) => debug!("数据库连接池健康检查通过"),
                            Err(e) => {
                                let metrics = pool_clone.get_metrics();
                                let status = pool_clone.get_pool_status();
                                error!(
                                    "数据库连接池健康检查失败: {} (活跃: {}, 空闲: {}, 等待: {}, 池大小: {})",
                                    e, metrics.active_connections, metrics.idle_connections,
                                    metrics.waiting_requests, status.size
                                );
                            }
                        }

                        pool_clone.update_metrics();
                        pool_clone.check_connection_leak();

                        let metrics = pool_clone.get_metrics();
                        let status = pool_clone.get_pool_status();
                        if metrics.waiting_requests > 0 || (status.size > 0 && metrics.active_connections as f32 / status.size as f32 > 0.8) {
                            info!(
                                "连接池状态 - 活跃: {}, 空闲: {}, 等待: {}, 池大小: {}",
                                metrics.active_connections, metrics.idle_connections,
                                metrics.waiting_requests, status.size
                            );
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("数据库连接池健康检查任务收到关闭信号，停止运行");
                        break;
                    }
                }
            }
        });
    }

    pub async fn close(&self) {
        let pool = self.get_pool();
        pool.close().await;
        info!("数据库连接池已关闭");
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolStatus {
    pub size: u32,
    pub num_idle: u32,
    pub is_closed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_metrics() {
        let metrics = PoolMetrics::new();

        metrics.record_request_start();
        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.waiting_requests.load(Ordering::Relaxed), 1);

        metrics.record_request_complete(100, true);
        assert_eq!(metrics.waiting_requests.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.avg_wait_time_ms.load(Ordering::Relaxed), 100);

        metrics.update_connection_counts(5, 3);
        assert_eq!(metrics.active_connections.load(Ordering::Relaxed), 5);
        assert_eq!(metrics.idle_connections.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.acquire_timeout_secs, 15);
        assert!(!config.auto_scaling_enabled);
    }

    #[test]
    fn test_pool_config_validation() {
        let valid_config = PoolConfig::default();
        assert!(valid_config.validate().is_ok());

        let invalid_config = PoolConfig {
            max_connections: 0,
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());

        let invalid_config2 = PoolConfig {
            min_connections: 100,
            max_connections: 10,
            ..Default::default()
        };
        assert!(invalid_config2.validate().is_err());

        let invalid_config3 = PoolConfig {
            max_lifetime_secs: 60,
            idle_timeout_secs: 120,
            ..Default::default()
        };
        assert!(invalid_config3.validate().is_err());
    }

    #[test]
    fn test_pool_config_min_connections_validation() {
        let db_config = DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            max_connections: 10,
            min_connections: 5,
            acquire_timeout_secs: 15,
            idle_timeout_secs: 60,
            max_lifetime_secs: 1800,
            query_timeout_secs: 30,
            slow_query_threshold_ms: 1000,
            health_check_interval_secs: 30,
        };
        let pool_config = PoolConfig::from(&db_config);
        assert_eq!(pool_config.min_connections, 5);
        assert!(pool_config.min_connections <= pool_config.max_connections);
        assert!(!pool_config.auto_scaling_enabled);
    }
}
