use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS cabinets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID REFERENCES rooms(id) ON DELETE RESTRICT,
            capacity INTEGER NOT NULL DEFAULT 42,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query(
        "ALTER TABLE cabinets ADD CONSTRAINT uq_cabinets_room_name UNIQUE (room_id, name)",
    )
    .execute(pool)
    .await
    {
        warn!("cabinets复合唯一约束可能已存在: {}", e);
    }

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

    Ok(())
}
