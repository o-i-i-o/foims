//! 初始化状态与数据库检查接口。

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use foims_common::msg;
use sqlx::PgPool;

use crate::check::{check_has_data, check_required_tables_exist};
use crate::context::InitContext;
use crate::error::InitError;

fn json_ok(value: serde_json::Value) -> Response {
    (StatusCode::OK, Json(value)).into_response()
}

pub async fn check_db_status(State(ctx): State<Arc<InitContext>>) -> Result<Response, InitError> {
    // 纯只读探测：GET 状态端点不携带建库/建 schema 副作用（与
    // check_init_status 口径一致）。先连 postgres 库判断目标库是否存在，
    // 目标库存在时再直连目标库做只读 schema 检查
    let cfg = ctx.db_config();
    let postgres_url = crate::utils::build_pg_url(&cfg, "postgres");

    let not_connected = || {
        // 错误详情（含连接 host/user）只入日志，不回传客户端（I-7）
        json_ok(serde_json::json!({
            "success": true,
            "data": {
                "connected": false,
                "has_tables": false,
                "required_tables_exist": false,
                "has_data": false,
                "error": "server.init.db.connect_failed"
            }
        }))
    };

    let postgres_pool = match PgPool::connect(&postgres_url).await {
        Ok(p) => p,
        Err(e) => {
            foims_common::log_warn!(
                "log.init.db.status_check_failed",
                error = msg("server.init.db.connect_failed")
                    .with("error", e)
                    .log_string()
            );
            return Ok(not_connected());
        }
    };

    // 目标库存在性只读检查（pg_database 系统目录查询，无副作用）
    let db_exists: bool =
        match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&cfg.database)
            .fetch_one(&postgres_pool)
            .await
        {
            Ok(exists) => exists,
            Err(e) => {
                postgres_pool.close().await;
                foims_common::log_warn!(
                    "log.init.db.status_check_failed",
                    error = msg("server.init.db.check_failed")
                        .with("error", e)
                        .log_string()
                );
                return Ok(not_connected());
            }
        };

    postgres_pool.close().await;

    if !db_exists {
        // 目标库尚不存在：返回未连接状态，由初始化页提示"将自动创建数据库"
        return Ok(not_connected());
    }

    // 目标库存在：直连目标库做只读 schema 检查（连接串密码已 URL 编码）
    let db_url = crate::utils::build_pg_url(&cfg, &cfg.database);
    let pool = match PgPool::connect(&db_url).await {
        Ok(p) => p,
        Err(e) => {
            foims_common::log_warn!(
                "log.init.db.status_check_failed",
                error = msg("server.init.db.connect_failed")
                    .with("error", e)
                    .log_string()
            );
            return Ok(not_connected());
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

    // 查询失败向上传播：状态探测不应把数据库故障误报成"无数据"
    let has_data = if required_tables_exist {
        check_has_data(&pool).await?
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
    // 只读探测：GET 状态端点不得携带建库/建 schema 的副作用，
    // 因此不再调用 ensure_database_and_schema
    if !ctx.init_enabled() {
        return Ok(json_ok(serde_json::json!({
            "initialized": false,
            "version": env!("CARGO_PKG_VERSION"),
        })));
    }

    // init 启用时目标库通常尚不存在：只做只读连接探测，
    // 连接失败与 users 表缺失同样视为未初始化
    let cfg = ctx.db_config();
    let db_url = crate::utils::build_pg_url(&cfg, &cfg.database);

    let initialized = match PgPool::connect(&db_url).await {
        // 状态探测保持宽容语义：users 表查询失败视为未初始化
        Ok(pool) => check_has_data(&pool).await.unwrap_or_default(),
        Err(_) => false,
    };

    Ok(json_ok(serde_json::json!({
        "initialized": initialized,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

pub async fn restart_program(State(ctx): State<Arc<InitContext>>) -> Result<Response, InitError> {
    // 重启端点仅在「初始化刚完成」后放行一次：init 模式开启期间与关闭后
    // 均拒绝，避免被滥用为任意重启入口
    if !ctx.consume_restart_arm() {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }
    foims_common::log_info!("log.init.restart_requested");
    let mode = (ctx.restart_fn)()
        .await
        .map_err(|e| InitError::Internal(msg("server.init.restart_failed").with("error", e)))?;

    // 纯 systemd 重启；未注册单元（程序并非以服务运行）不下发重启，
    // 由前端展示完整页面引导用户手动重启完成初始化
    let (message, restart_mode) = match mode {
        crate::context::RestartMode::Systemd => ("server.init.restart_command_sent", "systemd"),
        crate::context::RestartMode::Manual => ("server.init.manual_restart_required", "manual"),
    };
    Ok(json_ok(serde_json::json!({
        "success": true,
        "message": message,
        "data": { "restart_mode": restart_mode },
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
            foims_common::log_warn!("log.init.psql_check_failed", error = e);
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

    // 密码做 URL 编码后再拼连接串：含 @ : / 等字符时裸拼会导致误报（I-8）
    let cfg = ctx.db_config();
    let url = crate::utils::build_pg_url(&cfg, "postgres");

    match PgPool::connect(&url).await {
        Ok(_) => Ok(json_ok(serde_json::json!({
            "installed": true,
            "running": true,
            "message": "server.init.pgsql_running"
        }))),
        Err(e) => {
            // 错误串可能含连接串片段（host/用户名），只入日志不回传（I-7）
            let error_str = e.to_string();
            foims_common::log_warn!("log.init.pgsql_connect_failed", error = error_str);
            let running = !error_str.contains("connect")
                && !error_str.contains("timeout")
                && !error_str.contains("refused");

            Ok(json_ok(serde_json::json!({
                "installed": true,
                "running": running,
                "error": "server.init.pgsql_connect_failed"
            })))
        }
    }
}
