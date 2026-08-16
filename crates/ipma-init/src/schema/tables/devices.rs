//! 设备（devices）表结构创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS devices (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            device_type VARCHAR(30) NOT NULL DEFAULT 'other',
            brand VARCHAR(50),
            model VARCHAR(100),
            serial_number VARCHAR(100),
            workstation_id UUID REFERENCES workstations(id) ON DELETE SET NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            template_id UUID REFERENCES device_templates(id) ON DELETE SET NULL,
            vendor VARCHAR(50),
            location VARCHAR(100),
            snmp_version VARCHAR(3) DEFAULT 'v2c',
            snmp_community VARCHAR(64),
            snmp_username VARCHAR(22),
            snmp_auth_protocol VARCHAR(10),
            snmp_auth_password VARCHAR(100),
            snmp_priv_protocol VARCHAR(10),
            snmp_priv_password VARCHAR(100),
            snmp_port INTEGER DEFAULT 161,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN (
                'pc', 'laptop', 'printer', 'server', 'network_device', 'switch',
                'camera', 'phone', 'other'
            ))
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_devices_room_id ON devices(room_id)")
        .execute(pool)
        .await?;

    Ok(())
}
