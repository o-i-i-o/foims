//! 用户（users）与密码历史（password_history）表结构创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) UNIQUE NOT NULL,
            password_hash VARCHAR(255) NOT NULL,
            email VARCHAR(100) UNIQUE NOT NULL,
            role VARCHAR(20) NOT NULL,
            status BOOLEAN NOT NULL DEFAULT TRUE,
            auth_provider VARCHAR(20) NOT NULL DEFAULT 'local',
            reset_token VARCHAR(255),
            reset_token_expiry TIMESTAMP WITH TIME ZONE,
            two_factor_secret VARCHAR(255),
            two_factor_enabled BOOLEAN NOT NULL DEFAULT FALSE,
            two_factor_verified BOOLEAN NOT NULL DEFAULT FALSE,
            two_factor_email_code VARCHAR(10),
            two_factor_email_code_expiry TIMESTAMP WITH TIME ZONE,
            tokens_invalidated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            password_changed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    // 等保密码策略：历史密码哈希（改密时校验不得重复使用）
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS password_history (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            password_hash VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_password_history_user_id ON password_history(user_id, created_at DESC)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
