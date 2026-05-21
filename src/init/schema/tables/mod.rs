mod cabinets;
mod element;
mod encryption;
mod indexes;
mod ips;
mod logs;
mod network;
mod notifications;
mod rooms;
mod switches;
mod system;
mod tokens;
mod triggers;
mod users;
mod views;
mod workstations;

pub async fn create_all_tables(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"")
        .execute(pool)
        .await?;

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

    users::create(pool).await?;
    network::create(pool).await?;
    rooms::create(pool).await?;
    switches::create(pool).await?;
    cabinets::create(pool).await?;
    workstations::create(pool).await?;
    ips::create(pool).await?;
    logs::create(pool).await?;
    tokens::create(pool).await?;
    notifications::create(pool).await?;
    system::create(pool).await?;
    element::create(pool).await?;
    encryption::create(pool).await?;

    indexes::create(pool).await?;
    views::create(pool).await?;
    triggers::create(pool).await?;

    Ok(())
}
