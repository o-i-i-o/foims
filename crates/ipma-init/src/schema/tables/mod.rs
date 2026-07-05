mod access_points;
mod cabinets;
mod device_templates;
mod devices;
mod element;
mod encryption;
mod indexes;
mod ips;
mod logs;
mod network;
mod nodes;
mod notifications;
mod org_templates;
mod organizations;
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

    users::create(pool).await?;
    network::create(pool).await?;
    nodes::create(pool).await?;
    rooms::create(pool).await?;
    cabinets::create(pool).await?;
    workstations::create(pool).await?;
    device_templates::create(pool).await?;
    access_points::create(pool).await?;
    devices::create(pool).await?;
    switches::create(pool).await?;
    ips::create(pool).await?;
    logs::create(pool).await?;
    tokens::create(pool).await?;
    notifications::create(pool).await?;
    system::create(pool).await?;
    element::create(pool).await?;
    encryption::create(pool).await?;
    org_templates::create(pool).await?;
    organizations::create(pool).await?;

    indexes::create(pool).await?;
    views::create(pool).await?;
    triggers::create(pool).await?;

    Ok(())
}
