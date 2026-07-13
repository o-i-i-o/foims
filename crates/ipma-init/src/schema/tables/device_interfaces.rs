pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_interfaces (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            name VARCHAR(50) NOT NULL,
            interface_type VARCHAR(20) NOT NULL CHECK (interface_type IN (
                'physical', 'svi', 'management', 'loopback', 'wifi'
            )),
            mac_address VARCHAR(20),
            vlan_id INTEGER,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, name)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
