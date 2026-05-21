pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS workstations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(50) NOT NULL,
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            room_network_id UUID REFERENCES room_networks(id),
            manager VARCHAR(50),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_workstations_room_network_id ON workstations(room_network_id)"
    )
    .execute(pool)
    .await
    {
        tracing::warn!("创建 idx_workstations_room_network_id 索引失败: {}", e);
    }

    Ok(())
}
