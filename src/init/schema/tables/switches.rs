pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS switches (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            position_id UUID REFERENCES positions(id) ON DELETE SET NULL,
            model VARCHAR(100),
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
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS switch_ports (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            port_number VARCHAR(30) NOT NULL,
            port_name VARCHAR(50),
            port_type VARCHAR(20) DEFAULT 'access',
            vlan_id INTEGER,
            status VARCHAR(20) DEFAULT 'up',
            speed VARCHAR(20),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, port_number)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS switch_macs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            ip_address INET NOT NULL,
            mac_address VARCHAR(20) NOT NULL,
            interface VARCHAR(50),
            vlan_id INTEGER,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, ip_address)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS switch_lldps (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            switch_id UUID NOT NULL REFERENCES switches(id) ON DELETE CASCADE,
            local_port VARCHAR(50) NOT NULL,
            neighbor_chassis_id VARCHAR(100),
            neighbor_port_id VARCHAR(100),
            neighbor_port_desc VARCHAR(255),
            neighbor_sys_name VARCHAR(255),
            neighbor_sys_desc TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(switch_id, local_port)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
