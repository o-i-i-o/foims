use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS rooms (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_type VARCHAR(20) NOT NULL DEFAULT 'OFFICE',
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_room_type CHECK (room_type IN ('OFFICE', 'DATA_CENTER'))
        )",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("ALTER TABLE rooms ADD CONSTRAINT uq_rooms_name UNIQUE (name)")
        .execute(pool)
        .await
    {
        warn!("rooms.name唯一约束可能已存在: {}", e);
    }

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS room_networks (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
            network_id UUID REFERENCES network_cidrs(id),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(room_id, network_id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS workstation_layouts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
            element_id UUID NOT NULL,
            element_type VARCHAR(20) NOT NULL DEFAULT 'workstation',
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(room_id, element_id)
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
    .await
    .map(|_| ())
}
