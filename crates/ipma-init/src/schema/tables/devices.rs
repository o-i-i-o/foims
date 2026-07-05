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
            access_point_id UUID REFERENCES access_points(id) ON DELETE SET NULL,
            switch_port_id UUID REFERENCES switch_ports(id) ON DELETE SET NULL,
            template_id UUID REFERENCES device_templates(id) ON DELETE SET NULL,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_device_type CHECK (device_type IN (
                'pc', 'laptop', 'printer', 'server', 'network_device',
                'camera', 'phone', 'ap', 'other'
            )),
            CONSTRAINT chk_device_location CHECK (
                (workstation_id IS NOT NULL AND position_id IS NULL) OR
                (workstation_id IS NULL AND position_id IS NOT NULL) OR
                (workstation_id IS NULL AND position_id IS NULL)
            ),
            CONSTRAINT chk_device_connection CHECK (
                NOT (access_point_id IS NOT NULL AND switch_port_id IS NOT NULL)
            )
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
