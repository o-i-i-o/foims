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
            net_outlet_id UUID REFERENCES net_outlets(id) ON DELETE SET NULL,
            device_port_id UUID,
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
            )),
            CONSTRAINT chk_device_location CHECK (
                (workstation_id IS NOT NULL AND position_id IS NULL) OR
                (workstation_id IS NULL AND position_id IS NOT NULL) OR
                (workstation_id IS NULL AND position_id IS NULL)
            ),
            CONSTRAINT chk_device_connection CHECK (
                NOT (net_outlet_id IS NOT NULL AND device_port_id IS NOT NULL)
            )
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn add_foreign_keys(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"DO $$ BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM information_schema.table_constraints
                WHERE constraint_name = 'fk_devices_device_port_id'
            ) THEN
                ALTER TABLE devices ADD CONSTRAINT fk_devices_device_port_id
                    FOREIGN KEY (device_port_id) REFERENCES device_ports(id) ON DELETE SET NULL;
            END IF;
        END $$",
    )
    .execute(pool)
    .await?;

    Ok(())
}
