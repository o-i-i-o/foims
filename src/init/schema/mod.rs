mod migrations;
mod tables;

use sqlx::Error;
use sqlx::PgPool;

pub async fn create_tables(pool: &PgPool) -> Result<(), Error> {
    tables::create_all_tables(pool).await?;
    migrations::run_all(pool).await?;
    Ok(())
}

pub async fn run_migrations_only(pool: &PgPool) -> Result<(), Error> {
    sqlx::query(
        r"CREATE TABLE IF NOT EXISTS schema_migrations (
            version VARCHAR(50) PRIMARY KEY,
            applied_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
            description TEXT
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())?;

    migrations::run_all(pool).await?;
    Ok(())
}
