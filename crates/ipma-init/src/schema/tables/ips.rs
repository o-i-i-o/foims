pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS ips (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_interface_id UUID NOT NULL REFERENCES device_interfaces(id) ON DELETE SET NULL,
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
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
            CONSTRAINT uq_ips_ip_address UNIQUE (ip_address)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
