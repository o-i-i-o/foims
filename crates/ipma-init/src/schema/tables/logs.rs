pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS operation_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            user_id UUID REFERENCES users(id) ON DELETE SET NULL,
            action VARCHAR(100) NOT NULL,
            resource_type VARCHAR(50) NOT NULL,
            resource_id UUID,
            details JSONB NOT NULL DEFAULT '{}',
            result BOOLEAN NOT NULL,
            ip_address VARCHAR(50) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS task_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            task_name VARCHAR(100) NOT NULL,
            status VARCHAR(20) NOT NULL,
            details JSONB NOT NULL DEFAULT '{}',
            start_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            end_time TIMESTAMP WITH TIME ZONE,
            duration INTEGER
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS login_logs (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            username VARCHAR(50) NOT NULL,
            ip_address VARCHAR(50) NOT NULL,
            user_agent VARCHAR(255),
            success BOOLEAN NOT NULL,
            error_message VARCHAR(255),
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
