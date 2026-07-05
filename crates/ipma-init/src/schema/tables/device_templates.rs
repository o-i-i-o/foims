pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS device_templates (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL UNIQUE,
            device_type VARCHAR(30) NOT NULL DEFAULT 'other',
            brand VARCHAR(50),
            model VARCHAR(100),
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_dt_device_type CHECK (device_type IN (
                'pc', 'laptop', 'printer', 'server', 'network_device',
                'camera', 'phone', 'ap', 'other'
            ))
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
