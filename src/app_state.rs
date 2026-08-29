//! 应用共享状态（数据库连接池、配置、JWT 工具等）。

use std::sync::Arc;

use async_trait::async_trait;

use ipma_auth::provider::AuthProvider;
use ipma_auth::utils::JwtUtils;
use ipma_common::ArcSwap;
use ipma_common::SharedConfig;
use ipma_common::config::Config;
use ipma_common::crypto::{decrypt_password_async, encrypt_password_async};
use ipma_common::db::DbPool;
use ipma_common::rate_limit::RateLimiter;
use ipma_common::{AppError, DbProvider, msg};
use ipma_data_management::{DataError, DataProvider, DataResult, DatabaseConfig};
use ipma_scheduler::TaskRegistry;
use sqlx::PgPool;

use crate::shutdown::ShutdownSignal;

#[derive(Clone)]
pub struct AppState {
    /// 共享配置槽：五个写盘端点成功落盘后刷新（store），GET 类端点与
    /// src 侧读取一律经 `config_snapshot()` 取最新值，消除启动快照陈旧问题。
    pub config: SharedConfig,
    /// 进程启动时的配置快照。仅供 `AuthProvider::config()` trait 实现——
    /// 该 trait（ipma-auth，签名 `fn config(&self) -> &Config`）要求按引用
    /// 返回，无法从共享槽出借；其内部唯一读取点为重置链接的 public_url。
    startup_config: Config,
    pub pool: Option<DbPool>,
    pub jwt_utils: JwtUtils,
    pub task_registry: Arc<TaskRegistry>,
    /// 优雅关闭信号：register_service 等业务路径可请求本进程优雅退出
    /// （让位 systemd），与 OS 信号共用同一关闭流程。
    pub shutdown: ShutdownSignal,
    /// 请求限流器：预鉴权中间件按 IP 计数；已认证请求由鉴权中间件之后
    /// 的钩子经 `charge_user` 补记用户维度桶（见 routes/mod.rs）。
    pub rate_limiter: RateLimiter,
    /// 配置文件写锁：config.toml 的「读-改-写盘」端点（系统配置/语言/
    /// 会话与页面超时/恢复配置）必须互斥执行，防止并发写盘互相覆盖。
    /// 经 Arc 共享，Clone 后各实例指向同一把锁。
    pub config_write_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub fn new(
        config: Config,
        pool: Option<DbPool>,
        task_registry: Arc<TaskRegistry>,
        shutdown: ShutdownSignal,
        rate_limiter: RateLimiter,
    ) -> Result<Self, String> {
        let jwt_utils = JwtUtils::new(&config)?;
        Ok(Self {
            config: Arc::new(ArcSwap::from_pointee(config.clone())),
            startup_config: config,
            pool,
            jwt_utils,
            task_registry,
            shutdown,
            rate_limiter,
            config_write_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    /// 读取共享槽最新配置快照（写盘端点刷新后即为最新）。
    #[must_use]
    pub fn config_snapshot(&self) -> Arc<Config> {
        self.config.load_full()
    }

    pub fn pool(&self) -> Result<&DbPool, AppError> {
        self.pool
            .as_ref()
            .ok_or_else(|| AppError::Internal(msg("server.db.not_initialized")))
    }
}

impl DbProvider for AppState {
    fn pool(&self) -> Result<&DbPool, AppError> {
        AppState::pool(self)
    }
}

impl AuthProvider for AppState {
    fn jwt_utils(&self) -> &JwtUtils {
        &self.jwt_utils
    }

    fn config(&self) -> &Config {
        &self.startup_config
    }
}

#[async_trait]
impl DataProvider for AppState {
    fn pool(&self) -> DataResult<PgPool> {
        self.pool
            .as_ref()
            .map(|p| p.get_conn())
            .ok_or_else(|| DataError::Internal(msg("server.db.not_initialized")))
    }

    fn database_config(&self) -> DatabaseConfig {
        self.config_snapshot().database.clone()
    }

    async fn decrypt_password(&self, encrypted: &str) -> DataResult<String> {
        decrypt_password_async(encrypted.to_string())
            .await
            .map_err(|e| DataError::Internal(msg("server.common.decrypt_failed").with("error", e)))
    }

    async fn encrypt_password(&self, plain: &str) -> DataResult<String> {
        encrypt_password_async(plain.to_string())
            .await
            .map_err(|e| DataError::Internal(msg("server.common.encrypt_failed").with("error", e)))
    }
}
