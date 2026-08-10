pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS net_outlets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            outlet_type VARCHAR(20) NOT NULL DEFAULT 'wall_socket',
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            cabinet_id UUID REFERENCES cabinets(id) ON DELETE SET NULL,
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
