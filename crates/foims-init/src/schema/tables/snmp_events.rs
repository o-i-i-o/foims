//! SNMP 观测事件表（snmp_events）结构创建。
//!
//! 存储轮询与 MAC 同步路径产出的观测事件（链路翻转/设备重启/
//! ARP 增删冲突/未知终端），detail 为 JSONB 事件上下文；
//! 事件为事实源，通知仅是其派生出口。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS snmp_events (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            event_type VARCHAR(32) NOT NULL,
            interface VARCHAR(100),
            severity VARCHAR(10) NOT NULL DEFAULT 'info'
                CHECK (severity IN ('info', 'warning', 'critical')),
            detail JSONB NOT NULL DEFAULT '{}'::jsonb,
            occurred_at TIMESTAMP WITH TIME ZONE NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    // 设备维度时间线 + 事件类型维度过滤（列表查询两条主路径）
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_snmp_events_device_time \
         ON snmp_events (device_id, occurred_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_snmp_events_type_time \
         ON snmp_events (event_type, occurred_at DESC)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
