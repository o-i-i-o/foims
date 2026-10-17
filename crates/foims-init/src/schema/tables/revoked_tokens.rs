//! 令牌撤销表（revoked_tokens）结构创建。

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

    Ok(())
}
