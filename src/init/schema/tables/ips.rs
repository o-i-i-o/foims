pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS ips (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
            device_type VARCHAR(20) NOT NULL,
            network_id UUID REFERENCES network_cidrs(id),
            ip_address INET NOT NULL,
            ip_version SMALLINT NOT NULL DEFAULT 4,
            mac_address VARCHAR(20),
            hostname VARCHAR(100),
            status VARCHAR(20) NOT NULL DEFAULT 'active',
            last_seen TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            last_mac VARCHAR(20),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position', 'switch')),
            CONSTRAINT chk_device_consistency CHECK (
                (device_type = 'workstation' AND workstation_id IS NOT NULL AND position_id IS NULL) OR
                (device_type = 'cabinet_position' AND position_id IS NOT NULL AND workstation_id IS NULL) OR
                (device_type = 'switch' AND position_id IS NOT NULL AND workstation_id IS NULL)
            )
        )",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("CREATE INDEX IF NOT EXISTS idx_ips_network_id ON ips(network_id)")
        .execute(pool)
        .await
    {
        tracing::warn!("创建 idx_ips_network_id 索引失败: {}", e);
    }

    Ok(())
}
