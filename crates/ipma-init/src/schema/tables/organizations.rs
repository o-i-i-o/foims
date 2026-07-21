pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS organizations (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            type_path VARCHAR(50) NOT NULL DEFAULT '0',
            parent_id UUID REFERENCES organizations(id) ON DELETE RESTRICT,
            template_id UUID REFERENCES org_templates(id) ON DELETE SET NULL,
            level_index INT NOT NULL DEFAULT 0,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_organizations_parent_name UNIQUE (parent_id, name),
            CONSTRAINT chk_organizations_child_has_template CHECK (parent_id IS NULL OR template_id IS NOT NULL)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_organizations_parent_id ON organizations(parent_id)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_organizations_type_path ON organizations(type_path)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_organizations_template_id ON organizations(template_id)",
    )
    .execute(pool)
    .await?;

    // 触发器：确保level_index一致性
    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION trg_organizations_level_index()
        RETURNS TRIGGER AS $$
        BEGIN
            IF NEW.parent_id IS NULL THEN
                NEW.level_index := 0;
            ELSE
                SELECT level_index + 1 INTO NEW.level_index
                FROM organizations WHERE id = NEW.parent_id;
                IF NOT FOUND THEN
                    RAISE EXCEPTION 'Parent organization not found';
                END IF;
            END IF;
            RETURN NEW;
        END;
        $$ LANGUAGE plpgsql;
        ",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r"
        CREATE OR REPLACE TRIGGER organizations_level_index_trigger
        BEFORE INSERT OR UPDATE OF parent_id ON organizations
        FOR EACH ROW
        WHEN (pg_trigger_depth() = 0)
        EXECUTE FUNCTION trg_organizations_level_index();
        ",
    )
    .execute(pool)
    .await?;

    Ok(())
}
