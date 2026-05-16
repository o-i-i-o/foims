use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::RwLock as SyncRwLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info, warn};

use crate::config::DatabaseConfig;

#[derive(Debug, Default)]
pub struct PoolMetrics {
    pub active_connections: AtomicU32,
    pub idle_connections: AtomicU32,
    pub waiting_requests: AtomicU32,
    pub total_requests: AtomicU64,
    pub failed_requests: AtomicU64,
    pub avg_wait_time_ms: AtomicU64,
    pub last_updated: AtomicU64,
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

        let current_avg = self.avg_wait_time_ms.load(Ordering::Relaxed);
        let new_avg = if current_avg == 0 {
            wait_time_ms
        } else {
            (current_avg * 9 + wait_time_ms) / 10
        };
        self.avg_wait_time_ms.store(new_avg, Ordering::Relaxed);

        self.last_updated.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    pub fn update_connection_counts(&self, active: u32, idle: u32) {
        self.active_connections.store(active, Ordering::Relaxed);
        self.idle_connections.store(idle, Ordering::Relaxed);
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
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 2,
            acquire_timeout_secs: 5,
            idle_timeout_secs: 60,
            max_lifetime_secs: 1800,
            test_before_acquire: true,
            health_check_interval_secs: 30,
            auto_scaling_enabled: true,
            low_load_threshold: 0.3,
            high_load_threshold: 0.8,
            scaling_cooldown_secs: 60,
            query_timeout_secs: 30,
            slow_query_threshold_ms: 1000,
        }
    }
}

impl From<&DatabaseConfig> for PoolConfig {
    fn from(config: &DatabaseConfig) -> Self {
        let max_connections = config.max_connections;
        let min_connections = 2.min(max_connections);
        Self {
            max_connections,
            min_connections,
            acquire_timeout_secs: 5,
            idle_timeout_secs: 60,
            max_lifetime_secs: 1800,
            test_before_acquire: true,
            health_check_interval_secs: 30,
            auto_scaling_enabled: true,
            low_load_threshold: 0.3,
            high_load_threshold: 0.8,
            scaling_cooldown_secs: 60,
            query_timeout_secs: config.query_timeout_secs,
            slow_query_threshold_ms: config.slow_query_threshold_ms,
        }
    }
}

#[derive(Clone)]
pub struct DbPool {
    pool: Arc<SyncRwLock<PgPool>>,
    pub metrics: Arc<PoolMetrics>,
    pub config: Arc<RwLock<PoolConfig>>,
    pub db_config: DatabaseConfig,
    last_scaling_time: Arc<AtomicU64>,
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
        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.username, config.password, config.host, config.port, config.database
        );

        let pool = Self::create_pool(&url, &pool_config).await?;

        Ok(Self {
            pool: Arc::new(SyncRwLock::new(pool)),
            metrics: Arc::new(PoolMetrics::new()),
            config: Arc::new(RwLock::new(pool_config)),
            db_config: config.clone(),
            last_scaling_time: Arc::new(AtomicU64::new(0)),
        })
    }

    async fn create_pool(url: &str, config: &PoolConfig) -> Result<PgPool, sqlx::Error> {
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(std::time::Duration::from_secs(config.acquire_timeout_secs))
            .idle_timeout(Some(std::time::Duration::from_secs(
                config.idle_timeout_secs,
            )))
            .max_lifetime(Some(std::time::Duration::from_secs(
                config.max_lifetime_secs,
            )))
            .test_before_acquire(config.test_before_acquire)
            .connect(url)
            .await
    }

    fn build_database_url(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.db_config.username,
            self.db_config.password,
            self.db_config.host,
            self.db_config.port,
            self.db_config.database
        )
    }

    pub async fn acquire(
        &self,
    ) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, sqlx::Error> {
        let start_time = std::time::Instant::now();
        self.metrics.record_request_start();

        let pool = self.read_pool();
        let result = pool.acquire().await;
        let wait_time_ms = start_time.elapsed().as_millis() as u64;

        match &result {
            Ok(_) => {
                self.metrics.record_request_complete(wait_time_ms, true);
                debug!("获取连接成功，等待时间: {}ms", wait_time_ms);
            }
            Err(e) => {
                self.metrics.record_request_complete(wait_time_ms, false);
                error!("获取连接失败: {}, 等待时间: {}ms", e, wait_time_ms);
            }
        }

        self.update_metrics();

        result
    }

    #[must_use]
    pub fn get_conn(&self) -> PgPool {
        self.read_pool()
    }

    fn read_pool(&self) -> PgPool {
        self.pool
            .read()
            .unwrap_or_else(|e| {
                warn!("数据库连接池读锁中毒，自动恢复: {}", e);
                e.into_inner()
            })
            .clone()
    }

    pub async fn health_check(&self) -> Result<bool, sqlx::Error> {
        let pool = self.read_pool();
        sqlx::query("SELECT 1")
            .execute(&pool)
            .await
            .map(|_| true)
    }

    fn update_metrics(&self) {
        let pool = self.read_pool();
        let total = pool.size();
        let idle = pool.num_idle() as u32;
        let active = total.saturating_sub(idle);
        self.metrics.update_connection_counts(active, idle);
    }

    #[must_use]
    pub fn get_metrics(&self) -> PoolMetricsSnapshot {
        self.metrics.snapshot()
    }

    #[must_use]
    pub fn get_pool_status(&self) -> PoolStatus {
        let pool = self.read_pool();
        PoolStatus {
            size: pool.size(),
            num_idle: pool.num_idle() as u32,
            is_closed: pool.is_closed(),
        }
    }

    pub async fn resize_pool(&self, new_max_connections: u32) -> Result<(), sqlx::Error> {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut config = self.config.write().await;

        if current_time - self.last_scaling_time.load(Ordering::Relaxed)
            < config.scaling_cooldown_secs
        {
            warn!("连接池缩放冷却中，跳过本次调整");
            return Ok(());
        }

        let old_max = config.max_connections;
        config.max_connections = new_max_connections;
        config.min_connections = config.min_connections.min(new_max_connections);
        let new_config = config.clone();
        drop(config);

        let url = self.build_database_url();
        let new_pool = Self::create_pool(&url, &new_config).await?;

        let old_pool = {
            let mut pool_lock = self.pool.write().unwrap_or_else(|e| {
                warn!("数据库连接池写锁中毒，自动恢复: {}", e);
                e.into_inner()
            });
            std::mem::replace(&mut *pool_lock, new_pool)
        };

        tokio::spawn(async move {
            old_pool.close().await;
        });

        self.last_scaling_time
            .store(current_time, Ordering::Relaxed);

        self.update_metrics();

        warn!(
            "连接池已重建: 最大连接数 {} -> {}",
            old_max, new_max_connections
        );

        Ok(())
    }

    pub async fn check_and_scale(&self) {
        let config = self.config.read().await;

        if !config.auto_scaling_enabled {
            return;
        }

        let metrics = self.metrics.snapshot();
        let pool = self.read_pool();
        let pool_size = pool.size();

        if pool_size == 0 {
            return;
        }

        let utilization_rate = metrics.active_connections as f32 / pool_size as f32;
        let current_max = config.max_connections;
        let min_connections = config.min_connections;
        let high_load_threshold = config.high_load_threshold;
        let low_load_threshold = config.low_load_threshold;
        drop(config);

        if utilization_rate > high_load_threshold {
            let load_factor = utilization_rate / high_load_threshold;
            let growth_factor = (load_factor - 1.0).mul_add(0.3, 1.0);
            let new_max = (current_max as f32 * growth_factor)
                .min(100.0)
                .round()
                .clamp(0.0, u32::MAX as f32) as u32;

            if new_max > current_max + 1
                && let Err(e) = self.resize_pool(new_max).await
            {
                error!("连接池扩容失败: {}", e);
            }
        } else if utilization_rate < low_load_threshold {
            let load_factor = utilization_rate / low_load_threshold;
            let reduction_factor = load_factor.mul_add(0.2, 0.8);
            let new_max = (current_max as f32 * reduction_factor)
                .max(min_connections as f32)
                .round()
                .clamp(0.0, u32::MAX as f32) as u32;

            if new_max < current_max - 1
                && let Err(e) = self.resize_pool(new_max).await
            {
                error!("连接池缩容失败: {}", e);
            }
        }
    }

    pub fn start_health_check_task(&self, interval_seconds: u64) {
        let pool_clone = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(time::Duration::from_secs(interval_seconds));
            loop {
                interval.tick().await;

                match pool_clone.health_check().await {
                    Ok(_) => debug!("数据库连接池健康检查通过"),
                    Err(e) => error!("数据库连接池健康检查失败: {}", e),
                }

                pool_clone.update_metrics();

                pool_clone.check_and_scale().await;
            }
        });
    }

    pub fn start_metrics_collection_task(&self, interval_seconds: u64) {
        let pool_clone = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(time::Duration::from_secs(interval_seconds));
            loop {
                interval.tick().await;

                let metrics = pool_clone.get_metrics();
                let _status = pool_clone.get_pool_status();

                info!(
                    "连接池指标 - 活跃: {}, 空闲: {}, 等待: {}, 平均等待: {}ms, 总请求: {}, 失败: {}",
                    metrics.active_connections,
                    metrics.idle_connections,
                    metrics.waiting_requests,
                    metrics.avg_wait_time_ms,
                    metrics.total_requests,
                    metrics.failed_requests
                );
            }
        });
    }

    pub async fn close(&self) {
        let pool = self.read_pool();
        pool.close().await;
        info!("数据库连接池已关闭");
    }

    #[must_use]
    pub fn get_query_timeout(&self) -> std::time::Duration {
        let config = self
            .config
            .try_read()
            .map(|c| c.query_timeout_secs)
            .unwrap_or(30);
        std::time::Duration::from_secs(config)
    }

    #[must_use]
    pub fn get_slow_query_threshold_ms(&self) -> u64 {
        self.config
            .try_read()
            .map(|c| c.slow_query_threshold_ms)
            .unwrap_or(1000)
    }

    pub async fn execute_with_timeout<F, T>(
        &self,
        query_name: &str,
        future: F,
    ) -> Result<T, sqlx::Error>
    where
        F: std::future::Future<Output = Result<T, sqlx::Error>>,
    {
        let timeout = self.get_query_timeout();
        let slow_threshold = self.get_slow_query_threshold_ms();
        let start = std::time::Instant::now();

        self.metrics.record_request_start();

        let result = tokio::time::timeout(timeout, future).await;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        if elapsed_ms > slow_threshold {
            warn!(
                "慢查询警告: {} 耗时 {}ms (阈值: {}ms)",
                query_name, elapsed_ms, slow_threshold
            );
        }

        match result {
            Ok(Ok(value)) => {
                self.metrics.record_request_complete(elapsed_ms, true);
                self.update_metrics();
                Ok(value)
            }
            Ok(Err(e)) => {
                self.metrics.record_request_complete(elapsed_ms, false);
                self.update_metrics();
                Err(e)
            }
            Err(_) => {
                self.metrics.record_request_complete(elapsed_ms, false);
                self.update_metrics();
                Err(sqlx::Error::WorkerCrashed)
            }
        }
    }

    pub async fn query<F, T>(&self, query_name: &str, future: F) -> Result<T, sqlx::Error>
    where
        F: std::future::Future<Output = Result<T, sqlx::Error>>,
    {
        let slow_threshold = self.get_slow_query_threshold_ms();
        let start = std::time::Instant::now();

        self.metrics.record_request_start();

        let result = future.await;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        if elapsed_ms > slow_threshold {
            warn!(
                "慢查询警告: {} 耗时 {}ms (阈值: {}ms)",
                query_name, elapsed_ms, slow_threshold
            );
        }

        match result {
            Ok(value) => {
                self.metrics.record_request_complete(elapsed_ms, true);
                self.update_metrics();
                Ok(value)
            }
            Err(e) => {
                self.metrics.record_request_complete(elapsed_ms, false);
                self.update_metrics();
                Err(e)
            }
        }
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
        assert_eq!(config.min_connections, 2);
        assert_eq!(config.acquire_timeout_secs, 5);
        assert!(config.auto_scaling_enabled);
    }

    #[test]
    fn test_pool_config_min_connections_validation() {
        let db_config = DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            max_connections: 1,
            query_timeout_secs: 30,
            slow_query_threshold_ms: 1000,
        };
        let pool_config = PoolConfig::from(&db_config);
        assert_eq!(pool_config.min_connections, 1);
        assert!(pool_config.min_connections <= pool_config.max_connections);
    }
}
