//! 系统配置（system_configs）表结构创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS system_configs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            config_type VARCHAR(50) NOT NULL,
            key VARCHAR(100) NOT NULL,
            value TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(config_type, key)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS scheduled_tasks (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL UNIQUE,
            task_type VARCHAR(50) NOT NULL,
            cron_expression VARCHAR(100) NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            config JSONB NOT NULL DEFAULT '{}',
            last_run_at TIMESTAMP WITH TIME ZONE,
            next_run_at TIMESTAMP WITH TIME ZONE,
            last_result TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
