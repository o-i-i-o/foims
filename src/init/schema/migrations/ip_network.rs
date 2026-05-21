use sqlx::Error;
use sqlx::PgPool;
use sqlx::Row;

pub async fn run(pool: &PgPool) -> Result<(), Error> {
    migrate_fix_view_device_id(pool).await?;
    Ok(())
}

async fn migrate_fix_view_device_id(pool: &PgPool) -> Result<(), Error> {
    let result = sqlx::query(
        "SELECT COUNT(*) as count FROM schema_migrations WHERE version = 'fix_view_device_id_v2'",
    )
    .fetch_one(pool)
    .await?;

    let count: i64 = result.try_get("count").unwrap_or(0);

    if count == 0 {
        sqlx::query(
            r"INSERT INTO schema_migrations (version, description) VALUES ('fix_view_device_id_v2', '视图迁移已由 device_network.rs 处理')"
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
