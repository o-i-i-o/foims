pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS encryption_keys (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            key_name VARCHAR(50) NOT NULL UNIQUE,
            encryption_key TEXT NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}
