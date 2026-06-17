pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS workstations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            manager VARCHAR(50),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS workstation_layouts (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            workstation_id UUID NOT NULL REFERENCES workstations(id) ON DELETE CASCADE,
            x INTEGER NOT NULL DEFAULT 0,
            y INTEGER NOT NULL DEFAULT 0,
            width INTEGER NOT NULL DEFAULT 160,
            height INTEGER NOT NULL DEFAULT 160,
            rotation INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            UNIQUE(workstation_id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_workstation_layouts_workstation_id ON workstation_layouts(workstation_id)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
