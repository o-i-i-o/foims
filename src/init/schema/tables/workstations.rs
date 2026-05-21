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

    Ok(())
}
