//! 主机采集 Agent（agents）与指标历史（agent_metrics_history）表结构创建。
//!
//! 设计 docs/agent-design.md §5.2：agents 由下载 API 创建 pending 记录
//! （machine_id 未知，首报激活时回填）；唯一性用 partial unique index
//! （仅对已回填行生效）。agent_metrics_history 存 JSONB 全量指标供
//! 详情页曲线渲染，保留期由调度任务清理。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS agents (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            machine_id TEXT,
            token_hash TEXT NOT NULL UNIQUE,
            label TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            hostname TEXT,
            ip TEXT,
            os TEXT,
            kernel TEXT,
            arch TEXT,
            agent_version TEXT,
            cpu_usage NUMERIC(5,2),
            mem_usage_pct NUMERIC(5,2),
            disk_usage_pct NUMERIC(5,2),
            max_temp NUMERIC(5,1),
            uptime_secs BIGINT,
            raw_metrics JSONB,
            first_seen TIMESTAMP WITH TIME ZONE,
            last_seen TIMESTAMP WITH TIME ZONE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    // 同机重复安装判重：machine_id 可空（下载时未知），仅对已回填行唯一
    sqlx::query(
        r"CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_machine_id
            ON agents (machine_id) WHERE machine_id IS NOT NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS agent_metrics_history (
            id BIGSERIAL PRIMARY KEY,
            agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
            collected_at TIMESTAMP WITH TIME ZONE NOT NULL,
            metrics JSONB NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    // 详情页曲线查询按 (agent_id, collected_at) 范围扫描
    sqlx::query(
        r"CREATE INDEX IF NOT EXISTS idx_agent_metrics_history_agent_time
            ON agent_metrics_history (agent_id, collected_at)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
