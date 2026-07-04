use actix_web::{HttpResponse, web};
use sqlx::PgPool;
use tracing::info;

use crate::check::{check_has_data, check_required_tables_exist};
use crate::connection::ensure_database_and_schema;
use crate::context::InitContext;
use crate::error::InitError;

pub async fn check_db_status(ctx: web::Data<InitContext>) -> Result<HttpResponse, InitError> {
    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": {
                    "connected": false,
                    "has_tables": false,
                    "required_tables_exist": false,
                    "has_data": false,
                    "error": e
                }
            })));
        }
    };

    let has_tables = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_one(&pool)
    .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    };

    let required_tables_exist = check_required_tables_exist(&pool).await;

    let has_data = if required_tables_exist {
        check_has_data(&pool).await
    } else {
        false
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "data": {
            "connected": true,
            "has_tables": has_tables,
            "required_tables_exist": required_tables_exist,
            "has_data": has_data
        }
    })))
}

pub async fn check_init_status(ctx: web::Data<InitContext>) -> Result<HttpResponse, InitError> {
    let Ok(pool) = ensure_database_and_schema(&ctx.db_config).await else {
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "initialized": false,
            "version": env!("CARGO_PKG_VERSION"),
        })));
    };

    let initialized = match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
    {
        Ok(count) => count > 0,
        Err(_) => false,
    };

    Ok(HttpResponse::Ok().json(serde_json::json!({
        "initialized": initialized,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

pub async fn restart_program(ctx: web::Data<InitContext>) -> Result<HttpResponse, InitError> {
    info!("收到重启程序请求，正在准备重启...");
    (ctx.restart_fn)().await.map_err(InitError::Internal)?;
    Ok(HttpResponse::Ok().json(crate::ApiResponse::<()> {
        success: true,
        message: "服务重启命令已发送，服务正在重启...".to_string(),
        data: None,
    }))
}

pub async fn check_pgsql(ctx: web::Data<InitContext>) -> Result<HttpResponse, InitError> {
    let installed = match tokio::process::Command::new("which")
        .arg("psql")
        .status()
        .await
    {
        Ok(s) => s.success(),
        Err(e) => {
            tracing::warn!("检查 psql 安装状态失败: {}", e);
            false
        }
    };

    if !installed {
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "installed": false,
            "running": false,
            "error": "PostgreSQL is not installed. Please install PostgreSQL first."
        })));
    }

    let url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        ctx.db_config.username, ctx.db_config.password, ctx.db_config.host, ctx.db_config.port
    );

    match PgPool::connect(&url).await {
        Ok(_) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "installed": true,
            "running": true,
            "message": "PostgreSQL is running and connection is successful."
        }))),
        Err(e) => {
            let error_str = e.to_string();
            let running = !error_str.contains("connect")
                && !error_str.contains("timeout")
                && !error_str.contains("refused");

            Ok(HttpResponse::Ok().json(serde_json::json!({
                "installed": true,
                "running": running,
                "error": error_str
            })))
        }
    }
}
