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
            -- 系统内置任务：由启动播种写入（与代码/配置同源），前端只读展示，
            -- 禁止通过任务管理接口修改/删除，避免代码与 DB 定义双轨不一致
            is_system BOOLEAN NOT NULL DEFAULT FALSE,
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
