use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::types::DatabaseConfig;

pub type RestartFuture = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

#[derive(Clone)]
pub struct InitContext {
    pub db_config: DatabaseConfig,
    pub config_path: String,
    pub init_enabled: bool,
    pub restart_fn: Arc<dyn Fn() -> RestartFuture + Send + Sync>,
}
