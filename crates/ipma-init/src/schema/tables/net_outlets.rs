pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS net_outlets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            outlet_type VARCHAR(20) NOT NULL DEFAULT 'wall_socket',
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            cabinet_id UUID REFERENCES cabinets(id) ON DELETE SET NULL,
            peer_net_outlet_id UUID REFERENCES net_outlets(id) ON DELETE SET NULL,
            device_port_id UUID,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_outlet_type CHECK (outlet_type IN ('wall_socket', 'patch_panel', 'wifi_ap', 'other')),
            CONSTRAINT uq_net_outlets_name UNIQUE (room_id, name)
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
                WHERE constraint_name = 'fk_net_outlets_device_port_id'
            ) THEN
                ALTER TABLE net_outlets ADD CONSTRAINT fk_net_outlets_device_port_id
                    FOREIGN KEY (device_port_id) REFERENCES device_ports(id) ON DELETE SET NULL;
            END IF;
        END $$",
    )
    .execute(pool)
    .await?;

    Ok(())
}
