use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::info;

pub async fn start_scheduler() -> Result<(), Box<dyn std::error::Error>> {
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

    // 启动调度器
    scheduler.start().await?;
    info!("Cron scheduler started successfully");

    Ok(())
}
