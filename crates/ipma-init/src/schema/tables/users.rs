pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) UNIQUE NOT NULL,
            password_hash VARCHAR(255) NOT NULL,
            email VARCHAR(100) UNIQUE NOT NULL,
            role VARCHAR(20) NOT NULL,
            status BOOLEAN NOT NULL DEFAULT TRUE,
            reset_token VARCHAR(255),
            reset_token_expiry TIMESTAMP WITH TIME ZONE,
            two_factor_secret VARCHAR(255),
            two_factor_enabled BOOLEAN NOT NULL DEFAULT FALSE,
            two_factor_verified BOOLEAN NOT NULL DEFAULT FALSE,
            two_factor_email_code VARCHAR(10),
            two_factor_email_code_expiry TIMESTAMP WITH TIME ZONE,
            tokens_invalidated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    // 兼容旧库：补充 tokens_invalidated_at 列（用于密码重置/权限变更后吊销历史令牌）
    sqlx::query(
        "ALTER TABLE users ADD COLUMN IF NOT EXISTS tokens_invalidated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()",
    )
    .execute(pool)
    .await
    .map(|_| ())
}
