//! 拓扑节点与连线表结构创建（设备可视化）。
//!
//! topology_connections 仅存储手动连线（物理示意）与逻辑连线（链路聚合）；
//! 设备间的真实物理连线由 cable_links 实时派生，不再落库。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS topology_nodes (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            x INTEGER NOT NULL DEFAULT 100,
            y INTEGER NOT NULL DEFAULT 100,
            width INTEGER NOT NULL DEFAULT 200,
            height INTEGER NOT NULL DEFAULT 100,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS topology_connections (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            source_device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            target_device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            source_device_port_id UUID REFERENCES device_interfaces(id) ON DELETE SET NULL,
            target_device_port_id UUID REFERENCES device_interfaces(id) ON DELETE SET NULL,
            label VARCHAR(100),
            auto_discovered BOOLEAN NOT NULL DEFAULT FALSE,
            connection_type VARCHAR(20) NOT NULL DEFAULT 'physical',
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_no_self_connection CHECK (source_device_id != target_device_id),
            CONSTRAINT chk_topology_connections_type CHECK (connection_type IN ('physical', 'logical'))
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS topology_connection_members (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            connection_id UUID NOT NULL REFERENCES topology_connections(id) ON DELETE CASCADE,
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            device_port_id UUID NOT NULL REFERENCES device_interfaces(id) ON DELETE CASCADE,
            side VARCHAR(10) NOT NULL CHECK (side IN ('source', 'target')),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_topology_connection_member_port UNIQUE (connection_id, device_port_id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_topology_nodes_device_id ON topology_nodes(device_id)",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_topology_connections_source ON topology_connections(source_device_id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_topology_connections_target ON topology_connections(target_device_id)")
        .execute(pool)
        .await?;

    // 同一对设备之间只允许一条逻辑连接（链路聚合）：以表达式部分索引
    // 兜底并发下的 check-then-insert（清单见 check.rs 必需索引）。
    // 物理连线同一对设备允许多条（不同端口组合），故仅对 logical 生效
    sqlx::query(
        r"CREATE UNIQUE INDEX IF NOT EXISTS uq_topology_connections_logical
           ON topology_connections (
             LEAST(source_device_id, target_device_id),
             GREATEST(source_device_id, target_device_id)
           )
           WHERE connection_type = 'logical'",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tcm_connection ON topology_connection_members(connection_id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tcm_device_port ON topology_connection_members(device_port_id)")
        .execute(pool)
        .await?;

    // 成员表仅承载逻辑连线（链路聚合）的成员端口，端口全域唯一：
    // 同一端口不得同时参与两条逻辑连线，约束兜底并发下的 check-then-insert
    //（预检见 ipma-visualization/topology.rs，冲突映射见必需索引清单）
    sqlx::query(
        r"CREATE UNIQUE INDEX IF NOT EXISTS uq_topology_connection_member_port_global
           ON topology_connection_members (device_port_id)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
