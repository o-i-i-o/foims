pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS access_points (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            ap_type VARCHAR(20) NOT NULL DEFAULT 'wall_socket',
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            cabinet_id UUID REFERENCES cabinets(id) ON DELETE SET NULL,
            peer_access_point_id UUID REFERENCES access_points(id) ON DELETE SET NULL,
            switch_port_id UUID,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_ap_type CHECK (ap_type IN ('wall_socket', 'patch_panel', 'wifi_ap', 'other')),
            CONSTRAINT uq_access_points_name UNIQUE (room_id, name)
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
                WHERE constraint_name = 'fk_access_points_switch_port_id'
            ) THEN
                ALTER TABLE access_points ADD CONSTRAINT fk_access_points_switch_port_id
                    FOREIGN KEY (switch_port_id) REFERENCES switch_ports(id) ON DELETE SET NULL;
            END IF;
        END $$",
    )
    .execute(pool)
    .await?;

    Ok(())
}
