//! 初始化状态与数据库检查接口。

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ipma_common::msg;
use sqlx::PgPool;

use crate::check::{check_has_data, check_required_tables_exist};
use crate::connection::ensure_database_and_schema;
use crate::context::InitContext;
use crate::error::InitError;

fn json_ok(value: serde_json::Value) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}

pub async fn check_db_status(State(ctx): State<Arc<InitContext>>) -> Result<Response, InitError> {
    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(json_ok(serde_json::json!({
                "success": true,
                "data": {
                    "connected": false,
                    "has_tables": false,
                    "required_tables_exist": false,
                    "has_data": false,
                    "error": e.log_string()
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

    Ok(json_ok(serde_json::json!({
        "success": true,
        "data": {
            "connected": true,
            "has_tables": has_tables,
            "required_tables_exist": required_tables_exist,
            "has_data": has_data
        }
    })))
}

pub async fn check_init_status(State(ctx): State<Arc<InitContext>>) -> Result<Response, InitError> {
    let Ok(pool) = ensure_database_and_schema(&ctx.db_config).await else {
        return Ok(json_ok(serde_json::json!({
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

    Ok(json_ok(serde_json::json!({
        "initialized": initialized,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

pub async fn restart_program(State(ctx): State<Arc<InitContext>>) -> Result<Response, InitError> {
    ipma_common::log_info!("log.init.restart_requested");
    (ctx.restart_fn)()
        .await
        .map_err(|e| InitError::Internal(msg("server.init.restart_failed").with("error", e)))?;
    Ok(json_ok(serde_json::json!({
        "success": true,
        "message": "server.init.restart_command_sent",
        "data": null,
    })))
}

pub async fn check_pgsql(State(ctx): State<Arc<InitContext>>) -> Result<Response, InitError> {
    let installed = match tokio::process::Command::new("which")
        .arg("psql")
        .status()
        .await
    {
        Ok(s) => s.success(),
        Err(e) => {
            ipma_common::log_warn!("log.init.psql_check_failed", error = e);
            false
        }
    };

    if !installed {
        return Ok(json_ok(serde_json::json!({
            "installed": false,
            "running": false,
            "error": "server.init.pgsql_not_installed"
        })));
    }

    let url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        ctx.db_config.username, ctx.db_config.password, ctx.db_config.host, ctx.db_config.port
    );

    match PgPool::connect(&url).await {
        Ok(_) => Ok(json_ok(serde_json::json!({
            "installed": true,
            "running": true,
            "message": "server.init.pgsql_running"
        }))),
        Err(e) => {
            let error_str = e.to_string();
            let running = !error_str.contains("connect")
                && !error_str.contains("timeout")
                && !error_str.contains("refused");

            Ok(json_ok(serde_json::json!({
                "installed": true,
                "running": running,
                "error": error_str
            })))
        }
    }
}
