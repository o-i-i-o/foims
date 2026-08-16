//! 信息点（net_outlets）表结构创建。

pub async fn create(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS net_outlets (
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
            name VARCHAR(100) NOT NULL,
            room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE RESTRICT,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            CONSTRAINT uq_net_outlets_name UNIQUE (room_id, name)
        )",
    )
    .execute(pool)
    .await?;

    Ok(())
}
