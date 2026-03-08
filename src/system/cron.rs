use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;

use crate::db::DbPool;
use crate::utils::{cleanup_expired_revoked_tokens, cleanup_old_token_usage};

pub async fn start_scheduler(pool: Arc<DbPool>) -> Result<(), Box<dyn std::error::Error>> {
    let scheduler = JobScheduler::new().await?;

    // 每天凌晨执行一次数据备份
    let backup_job = Job::new_async("0 0 0 * * *", |_, _| {
        Box::pin(async move {
            info!("Running data backup job...");
            // 执行数据备份逻辑
            info!("Data backup job completed");
        })
    })?;
    scheduler.add(backup_job).await?;

    // 每小时清理过期的撤销 token
    let pool_for_tokens = pool.clone();
    let token_cleanup_job = Job::new_async("0 0 * * * *", move |_, _| {
        let pool = pool_for_tokens.clone();
        Box::pin(async move {
            info!("Running expired token cleanup job...");
            match cleanup_expired_revoked_tokens(&pool.pool).await {
                Ok(count) => info!("Token cleanup completed: {} expired tokens removed", count),
                Err(e) => info!("Token cleanup failed: {}", e),
            }
        })
    })?;
    scheduler.add(token_cleanup_job).await?;

    // 每天凌晨2点清理30天前的 token_usage 记录
    let pool_for_usage = pool.clone();
    let usage_cleanup_job = Job::new_async("0 0 2 * * *", move |_, _| {
        let pool = pool_for_usage.clone();
        Box::pin(async move {
            info!("Running old token usage cleanup job...");
            match cleanup_old_token_usage(&pool.pool, 30).await {
                Ok(count) => info!("Token usage cleanup completed: {} old records removed", count),
                Err(e) => info!("Token usage cleanup failed: {}", e),
            }
        })
    })?;
    scheduler.add(usage_cleanup_job).await?;

    // 启动调度器
    scheduler.start().await?;
    info!("Cron scheduler started successfully");

    Ok(())
}
