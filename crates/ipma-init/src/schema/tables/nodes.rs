pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS nodes (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            node_type VARCHAR(20) NOT NULL,
            parent_id UUID REFERENCES nodes(id) ON DELETE RESTRICT,
            description TEXT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT chk_node_type CHECK (node_type IN ('campus', 'building', 'floor')),
            CONSTRAINT uq_nodes_parent_name UNIQUE (parent_id, name)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
