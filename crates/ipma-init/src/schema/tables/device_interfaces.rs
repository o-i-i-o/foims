pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_interfaces (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            device_id UUID NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
            network_card_id UUID REFERENCES network_cards(id) ON DELETE SET NULL,
            name VARCHAR(50) NOT NULL,
            interface_type VARCHAR(20) NOT NULL CHECK (interface_type IN (
                'physical', 'svi', 'management', 'loopback', 'wifi'
            )),
            mac_address VARCHAR(20),
            vlan_id INTEGER,
            description TEXT,
            switch_id UUID REFERENCES devices(id) ON DELETE SET NULL,
            uplink_interface_id UUID REFERENCES device_interfaces(id) ON DELETE SET NULL,
            net_outlet_ids UUID[] NOT NULL DEFAULT '{}',
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(device_id, name)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
