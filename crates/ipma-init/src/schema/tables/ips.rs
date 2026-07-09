pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS ips (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
            device_id UUID REFERENCES devices(id) ON DELETE CASCADE,
            device_type VARCHAR(20),
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
            CONSTRAINT chk_device_type CHECK (device_type IN ('workstation', 'cabinet_position', 'switch', 'device')),
            CONSTRAINT chk_ip_device_ref CHECK (
                device_id IS NOT NULL OR workstation_id IS NOT NULL OR position_id IS NOT NULL
            ),
            CONSTRAINT uq_ips_ip_address UNIQUE (ip_address)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
