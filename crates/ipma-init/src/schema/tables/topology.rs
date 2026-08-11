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
            source_device_port_id UUID REFERENCES device_ports(id) ON DELETE SET NULL,
            target_device_port_id UUID REFERENCES device_ports(id) ON DELETE SET NULL,
            label VARCHAR(100),
            auto_discovered BOOLEAN NOT NULL DEFAULT FALSE,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_no_self_connection CHECK (source_device_id != target_device_id),
            CONSTRAINT uq_topology_connection UNIQUE (source_device_id, target_device_id, source_device_port_id, target_device_port_id)
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

    Ok(())
}
