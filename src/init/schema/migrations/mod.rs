mod data_consistency;
mod layouts;
mod ip_network;
mod switches;

use sqlx::PgPool;
use sqlx::Error;

pub async fn run_all(pool: &PgPool) -> Result<(), Error> {
    switches::run(pool).await?;
    ip_network::run(pool).await?;
    layouts::run(pool).await?;
    data_consistency::run(pool).await?;
    Ok(())
}
