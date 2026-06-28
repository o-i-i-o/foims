pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS cabinets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID REFERENCES rooms(id) ON DELETE RESTRICT,
            capacity INTEGER NOT NULL DEFAULT 42,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_cabinets_room_name UNIQUE (room_id, name)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS positions (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            cabinet_id UUID REFERENCES cabinets(id) ON DELETE CASCADE,
            start_u INTEGER NOT NULL DEFAULT 1,
            end_u INTEGER NOT NULL DEFAULT 1,
            device_type VARCHAR(20) DEFAULT 'cabinet_position',
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_position_device_type CHECK (device_type IN ('cabinet_position', 'switch'))
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS cabinet_layouts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            cabinet_id UUID NOT NULL REFERENCES cabinets(id) ON DELETE CASCADE,
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(cabinet_id)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
