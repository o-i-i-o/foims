pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS revoked_tokens (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            token_hash VARCHAR(255) NOT NULL,
            user_id UUID REFERENCES users(id) ON DELETE CASCADE,
            revoked_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            expiry TIMESTAMP WITH TIME ZONE NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS token_usage (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            token_hash VARCHAR(255) NOT NULL,
            user_id UUID REFERENCES users(id) ON DELETE CASCADE,
            ip_address VARCHAR(50) NOT NULL,
            user_agent VARCHAR(255),
            request_path VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
}
