use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS org_templates (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL UNIQUE,
            levels JSONB NOT NULL,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
        )",
    )
    .execute(pool)
    .await?;

    if let Err(e) =
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_org_templates_name ON org_templates(name)")
            .execute(pool)
            .await
    {
        warn!("org_templates名称索引可能已存在: {}", e);
    }

    // 为 organizations 表添加 template_id 和 level_index 列
    if let Err(e) = sqlx::query(
        "ALTER TABLE organizations ADD COLUMN IF NOT EXISTS template_id UUID REFERENCES org_templates(id) ON DELETE SET NULL",
    )
    .execute(pool)
    .await
    {
        warn!("organizations添加template_id列可能已存在: {}", e);
    }

    if let Err(e) = sqlx::query(
        "ALTER TABLE organizations ADD COLUMN IF NOT EXISTS level_index INT NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await
    {
        warn!("organizations添加level_index列可能已存在: {}", e);
    }

    if let Err(e) = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_organizations_template_id ON organizations(template_id)",
    )
    .execute(pool)
    .await
    {
        warn!("organizations模板索引可能已存在: {}", e);
    }

    Ok(())
}
