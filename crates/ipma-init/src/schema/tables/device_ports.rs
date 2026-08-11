pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_ports (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            port_number VARCHAR(30) NOT NULL,
            port_name VARCHAR(50),
            port_type VARCHAR(20) NOT NULL DEFAULT 'access' CHECK (port_type IN ('access','trunk','uplink','stack','console')),
            vlan_id INTEGER,
            status VARCHAR(20) NOT NULL DEFAULT 'up',
            speed VARCHAR(20),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, port_number)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_macs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            ip_address INET NOT NULL,
            mac_address VARCHAR(20) NOT NULL,
            interface VARCHAR(50),
            vlan_id INTEGER,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, ip_address)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_lldps (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            local_port VARCHAR(50) NOT NULL,
            neighbor_chassis_id VARCHAR(100),
            neighbor_port_id VARCHAR(100),
            neighbor_port_desc VARCHAR(255),
            neighbor_sys_name VARCHAR(255),
            neighbor_sys_desc TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, local_port)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
