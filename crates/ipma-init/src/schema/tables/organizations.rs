pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS organizations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            org_type VARCHAR(50) NOT NULL,
            parent_id UUID REFERENCES organizations(id) ON DELETE RESTRICT,
            template_id UUID REFERENCES org_templates(id) ON DELETE SET NULL,
            level_index INT NOT NULL DEFAULT 0,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_organizations_parent_name UNIQUE (parent_id, name)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_organizations_parent_id ON organizations(parent_id)",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_organizations_org_type ON organizations(org_type)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_organizations_template_id ON organizations(template_id)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
