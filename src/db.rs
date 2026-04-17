use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tokio::sync::RwLock;
use tokio::time;
use tracing::{debug, error, info, warn};

use crate::config::DatabaseConfig;

/// 连接池性能指标
#[derive(Debug, Default)]
pub struct PoolMetrics {
    /// 活跃连接数
    pub active_connections: AtomicU32,
    /// 空闲连接数
    pub idle_connections: AtomicU32,
    /// 等待获取连接的请求数
    pub waiting_requests: AtomicU32,
    /// 总请求数
    pub total_requests: AtomicU64,
    /// 失败请求数
    pub failed_requests: AtomicU64,
    /// 平均获取连接等待时间（毫秒）
    pub avg_wait_time_ms: AtomicU64,
    /// 最后更新时间
    pub last_updated: AtomicU64,
}

impl PoolMetrics {
    pub fn new() -> Self {
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

    /// 记录请求开始
    pub fn record_request_start(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.waiting_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录请求完成
    pub fn record_request_complete(&self, wait_time_ms: u64, success: bool) {
        self.waiting_requests.fetch_sub(1, Ordering::Relaxed);

        if !success {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }

        // 使用指数移动平均计算平均等待时间
        let current_avg = self.avg_wait_time_ms.load(Ordering::Relaxed);
        let new_avg = if current_avg == 0 {
            wait_time_ms
        } else {
            (current_avg * 9 + wait_time_ms) / 10
        };
        self.avg_wait_time_ms.store(new_avg, Ordering::Relaxed);

        // 更新最后更新时间
        self.last_updated.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            Ordering::Relaxed,
        );
    }

    /// 更新连接数
    pub fn update_connection_counts(&self, active: u32, idle: u32) {
        self.active_connections.store(active, Ordering::Relaxed);
        self.idle_connections.store(idle, Ordering::Relaxed);
    }

    /// 获取当前指标快照
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

/// 连接池指标快照
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

/// 连接池配置选项
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// 最大连接数
    pub max_connections: u32,
    /// 最小连接数
    pub min_connections: u32,
    /// 获取连接超时时间（秒）
    pub acquire_timeout_secs: u64,
    /// 空闲连接超时时间（秒）
    pub idle_timeout_secs: u64,
    /// 连接最大生命周期（秒）
    pub max_lifetime_secs: u64,
    /// 是否在获取前测试连接
    pub test_before_acquire: bool,
    /// 健康检查间隔（秒）
    pub health_check_interval_secs: u64,
    /// 是否启用自动缩放
    pub auto_scaling_enabled: bool,
    /// 连接池低负载阈值（活跃连接比例）
    pub low_load_threshold: f32,
    /// 连接池高负载阈值（活跃连接比例）
    pub high_load_threshold: f32,
    /// 最小缩放间隔（秒）
    pub scaling_cooldown_secs: u64,
    /// 查询超时时间（秒）
    pub query_timeout_secs: u64,
    /// 慢查询阈值（毫秒）
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
        Self {
            max_connections: config.max_connections,
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
            query_timeout_secs: config.query_timeout_secs,
            slow_query_threshold_ms: config.slow_query_threshold_ms,
        }
    }
}

/// 数据库连接池包装器
#[derive(Clone)]
pub struct DbPool {
    pub pool: PgPool,
    pub metrics: Arc<PoolMetrics>,
    pub config: Arc<RwLock<PoolConfig>>,
    pub db_config: DatabaseConfig,
    last_scaling_time: Arc<AtomicU64>,
}

impl DbPool {
    /// 创建新的连接池
    pub async fn new(config: &DatabaseConfig) -> Result<Self, sqlx::Error> {
        let pool_config = PoolConfig::from(config);
        Self::new_with_config(config, pool_config).await
    }

    /// 使用自定义配置创建连接池
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
            pool,
            metrics: Arc::new(PoolMetrics::new()),
            config: Arc::new(RwLock::new(pool_config)),
            db_config: config.clone(),
            last_scaling_time: Arc::new(AtomicU64::new(0)),
        })
    }

    /// 创建连接池内部方法
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

    /// 获取数据库连接（带监控）
    pub async fn acquire(&self) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, sqlx::Error> {
        let start_time = std::time::Instant::now();
        self.metrics.record_request_start();

        let result = self.pool.acquire().await;
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

        // 更新连接数指标
        self.update_metrics().await;

        result
    }

    /// 获取连接引用
    pub fn get_conn(&self) -> &PgPool {
        &self.pool
    }

    /// 健康检查
    pub async fn health_check(&self) -> Result<bool, sqlx::Error> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map(|_| true)
    }

    /// 更新指标
    async fn update_metrics(&self) {
        let status = self.pool.size();
        self.metrics
            .update_connection_counts(status, self.pool.num_idle() as u32);
    }

    /// 获取当前指标快照
    pub fn get_metrics(&self) -> PoolMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// 获取连接池状态
    pub fn get_pool_status(&self) -> PoolStatus {
        PoolStatus {
            size: self.pool.size(),
            num_idle: self.pool.num_idle() as u32,
            is_closed: self.pool.is_closed(),
        }
    }

    /// 动态调整连接池大小
    /// 注意：SQLx 的 PgPool 创建后不支持动态调整大小，
    /// 此方法仅更新内部配置记录，实际连接池大小需要重启服务才能生效。
    pub async fn resize_pool(&self, new_max_connections: u32) -> Result<(), sqlx::Error> {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let last_scaling = self.last_scaling_time.load(Ordering::Relaxed);
        let config = self.config.read().await;

        if current_time - last_scaling < config.scaling_cooldown_secs {
            warn!("连接池缩放冷却中，跳过本次调整");
            return Ok(());
        }
        drop(config);

        let mut config = self.config.write().await;
        let old_max = config.max_connections;
        config.max_connections = new_max_connections;
        drop(config);

        self.last_scaling_time
            .store(current_time, Ordering::Relaxed);

        warn!(
            "连接池大小配置已更新: {} -> {}，但需要重启服务才能生效",
            old_max, new_max_connections
        );

        Ok(())
    }

    /// 自动缩放检查
    pub async fn check_and_scale(&self) {
        let config = self.config.read().await;

        if !config.auto_scaling_enabled {
            return;
        }

        let metrics = self.metrics.snapshot();
        let total_connections = metrics.active_connections + metrics.idle_connections;

        if total_connections == 0 {
            return;
        }

        let utilization_rate = metrics.active_connections as f32 / total_connections as f32;
        let current_max = config.max_connections;
        let min_connections = config.min_connections;
        let high_load_threshold = config.high_load_threshold;
        let low_load_threshold = config.low_load_threshold;
        drop(config);

        // 高负载：增加连接数
        if utilization_rate > high_load_threshold {
            // 计算新的最大连接数，使用更保守的增长策略
            // 避免每次都增加50%，而是根据负载程度动态调整
            let load_factor = utilization_rate / high_load_threshold;
            let growth_factor = 1.0 + (load_factor - 1.0) * 0.3; // 最大增长30%
            let new_max = (current_max as f32 * growth_factor).min(100.0) as u32;

            // 只有当新的最大连接数比当前大至少2个时才进行调整
            if new_max > current_max + 1
                && let Err(e) = self.resize_pool(new_max).await
            {
                error!("连接池扩容失败: {}", e);
            }
        }
        // 低负载：减少连接数
        else if utilization_rate < low_load_threshold {
            // 计算新的最大连接数，使用更保守的减少策略
            // 避免每次都减少20%，而是根据负载程度动态调整
            let load_factor = utilization_rate / low_load_threshold;
            let reduction_factor = 0.8 + (load_factor * 0.2); // 最小减少20%
            let new_max =
                (current_max as f32 * reduction_factor).max(min_connections as f32) as u32;

            // 只有当新的最大连接数比当前小至少2个时才进行调整
            if new_max < current_max - 1
                && let Err(e) = self.resize_pool(new_max).await
            {
                error!("连接池缩容失败: {}", e);
            }
        }
    }

    /// 启动定期健康检查任务
    pub fn start_health_check_task(&self, interval_seconds: u64) {
        let pool_clone = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(time::Duration::from_secs(interval_seconds));
            loop {
                interval.tick().await;

                // 健康检查
                match pool_clone.health_check().await {
                    Ok(_) => debug!("数据库连接池健康检查通过"),
                    Err(e) => error!("数据库连接池健康检查失败: {}", e),
                }

                // 更新指标
                pool_clone.update_metrics().await;

                // 自动缩放检查
                pool_clone.check_and_scale().await;
            }
        });
    }

    /// 启动指标收集任务
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

    /// 关闭连接池
    pub async fn close(&self) {
        self.pool.close().await;
        info!("数据库连接池已关闭");
    }

    /// 获取查询超时时间
    pub fn get_query_timeout(&self) -> std::time::Duration {
        let config = self
            .config
            .try_read()
            .map(|c| c.query_timeout_secs)
            .unwrap_or(30);
        std::time::Duration::from_secs(config)
    }

    /// 获取慢查询阈值
    pub fn get_slow_query_threshold_ms(&self) -> u64 {
        self.config
            .try_read()
            .map(|c| c.slow_query_threshold_ms)
            .unwrap_or(1000)
    }

    /// 执行带超时的查询
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

        let result = tokio::time::timeout(timeout, future).await;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        // 记录慢查询
        if elapsed_ms > slow_threshold {
            warn!(
                "慢查询警告: {} 耗时 {}ms (阈值: {}ms)",
                query_name, elapsed_ms, slow_threshold
            );
        }

        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(sqlx::Error::PoolTimedOut),
        }
    }
}

/// 连接池状态
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
}
