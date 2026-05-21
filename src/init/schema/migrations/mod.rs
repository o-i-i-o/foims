mod data_consistency;
mod device_network;
mod ip_network;
mod layouts;
mod switches;

use sqlx::Error;
use sqlx::PgPool;

pub async fn run_all(pool: &PgPool) -> Result<(), Error> {
    switches::run(pool).await?;
    ip_network::run(pool).await?;
    layouts::run(pool).await?;
    data_consistency::run(pool).await?;
    device_network::run(pool).await?;
    Ok(())
}
