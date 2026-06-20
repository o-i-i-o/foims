use tracing::warn;

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS organizations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            org_type VARCHAR(30) NOT NULL,
            parent_id UUID REFERENCES organizations(id) ON DELETE RESTRICT,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_org_type CHECK (
                org_type IN (
                    'headquarters', 'building', 'floor', 'hall',
                    'office', 'data_center', 'workstation',
                    'cabinet', 'cabinet_position'
                )
            )
        )",
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query(
        "ALTER TABLE organizations ADD CONSTRAINT uq_organizations_parent_name UNIQUE (parent_id, name)",
    )
    .execute(pool)
    .await
    {
        warn!("organizations复合唯一约束可能已存在: {}", e);
    }

    if let Err(e) = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_organizations_parent_id ON organizations(parent_id)",
    )
    .execute(pool)
    .await
    {
        warn!("organizations父级索引可能已存在: {}", e);
    }

    if let Err(e) = sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_organizations_org_type ON organizations(org_type)",
    )
    .execute(pool)
    .await
    {
        warn!("organizations类型索引可能已存在: {}", e);
    }

    Ok(())
}
