//! 数据库结构（DDL）创建入口。

mod tables;

use sqlx::Error;
use sqlx::PgPool;

pub async fn create_tables(pool: &PgPool) -> Result<(), Error> {
    tables::create_all_tables(pool).await?;
    Ok(())
}
