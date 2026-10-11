//! 数据库连接建立与 schema 存在性保证。

use foims_common::{AppMessage, msg};
use sqlx::PgPool;

use crate::types::DatabaseConfig;
use crate::utils::build_pg_url;

use crate::operations::{quote_ident, restrict_public_grants, validate_identifier};

pub async fn ensure_database_and_schema(config: &DatabaseConfig) -> Result<PgPool, AppMessage> {
    validate_identifier(&config.database)?;

    let postgres_url = build_pg_url(config, "postgres");

    let postgres_pool = PgPool::connect(&postgres_url)
        .await
        .map_err(|e| msg("server.init.db.pgsql_connect_failed").with("error", e))?;

    let db_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&config.database)
            .fetch_one(&postgres_pool)
            .await
            .map_err(|e| msg("server.init.db.check_failed").with("error", e))?;

    if !db_exists {
        foims_common::log_info!("log.init.db.missing_creating", name = config.database);
        // 库名为已校验标识符（validate_identifier + quote_ident 加固），
        // CREATE DATABASE 不支持参数绑定，经 QueryBuilder push 拼接
        sqlx::QueryBuilder::<sqlx::Postgres>::new("CREATE DATABASE ")
            .push(quote_ident(&config.database))
            .build()
            .execute(&postgres_pool)
            .await
            .map_err(|e| msg("server.init.db.create_failed").with("error", e))?;
        // 补建路径同样收紧 PUBLIC 授权，失败处理与 operations::create_database
        // 的新建路径口径一致（致命错误）：REVOKE 失败意味着实例状态异常，
        // 留下「任意角色均可连接」的库继续初始化比中止流程更危险；
        // 与 init-pgsql.sh 的授权模型一致——除所有者外的授权一律不保留
        restrict_public_grants(&postgres_pool, &config.database)
            .await
            .map_err(|e| msg("server.init.db.revoke_public_failed").with("error", e))?;
        foims_common::log_info!("log.init.db.created", name = config.database);
    }

    postgres_pool.close().await;

    let db_url = build_pg_url(config, &config.database);

    let pool = PgPool::connect(&db_url)
        .await
        .map_err(|e| msg("server.init.db.connect_failed").with("error", e))?;

    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = 'public')",
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| msg("server.init.db.schema_check_failed").with("error", e))?;

    if !schema_exists {
        foims_common::log_info!("log.init.db.schema_missing_creating");
        sqlx::query("CREATE SCHEMA IF NOT EXISTS public")
            .execute(&pool)
            .await
            .map_err(|e| msg("server.init.db.schema_create_failed").with("error", e))?;
        if let Err(e) = sqlx::query("GRANT ALL ON SCHEMA public TO postgres")
            .execute(&pool)
            .await
        {
            foims_common::log_warn!("log.init.db.grant_postgres_failed", error = e);
        }
        // 仅授予 USAGE：把 CREATE 授予 PUBLIC 会让任意可连接用户在
        // public schema 建表（PG14 及以下默认即如此），与「应用账号
        // 独占目标库」的收紧原则冲突；建表权限由库所有者隐式持有
        if let Err(e) = sqlx::query("GRANT USAGE ON SCHEMA public TO public")
            .execute(&pool)
            .await
        {
            foims_common::log_warn!("log.init.db.grant_public_failed", error = e);
        }
        foims_common::log_info!("log.init.db.public_schema_created");
    }

    Ok(pool)
}
