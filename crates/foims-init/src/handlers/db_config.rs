//! 数据库配置页接口（连接测试与按页面输入创建数据库）。
//!
//! 供初始化向导第 2 步使用。两个端点均只在 init 模式开放，
//! 且经路由层 localhost-only 中间件限制为本机访问；不要求控制台
//! 验证码——连接测试无破坏性，创建数据库为幂等新建（已存在时
//! 跳过且绝不触碰实例上的其他数据库）。
//!
//! 写入时机约定：连接测试通过后才把连接五要素写入配置文件并切换
//! 内存连接配置；失败的参数不落盘，配置文件不会残留无效连接信息。

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::Response;
use foims_common::msg;
use sqlx::{Connection, PgConnection};
use validator::Validate;

use crate::config::update_config_database;
use crate::context::InitContext;
use crate::error::{InitError, ok_json};
use crate::operations::{CreateOutcome, classify_pg_connect_error, create_database};
use crate::types::{DatabaseConfig, DatabaseSetupRequest};
use crate::utils::build_pg_url;

/// 以单连接探测指定库的可达性，失败时按分类返回 Validation 错误
/// （分类 key 透传前端翻译；错误详情含连接串，只入日志不回传，I-7）
async fn probe_database(
    config: &DatabaseConfig,
    database: &str,
    stage: &str,
) -> Result<(), InitError> {
    let url = build_pg_url(config, database);
    let conn = PgConnection::connect(&url).await.map_err(|e| {
        let kind = classify_pg_connect_error(&e);
        foims_common::log_warn!(
            "log.init.db.test_failed",
            stage = stage,
            database = database,
            kind = kind.message_key(),
            error = e
        );
        InitError::Validation(msg(kind.message_key()))
    })?;
    // 探测成功后优雅关闭；关闭失败不影响探测结论，仅记日志
    if let Err(e) = conn.close().await {
        foims_common::log_warn!("log.init.db.probe_close_failed", error = e);
    }
    Ok(())
}

/// 连接测试（充当下一步按钮）：
///
/// 1. 连接 postgres 系统库——验证服务器可达与凭据，失败时区分
///    「服务器不可达」（检查地址/端口/防火墙）与「认证被拒」
///    （用户不存在或密码错误）；
/// 2. 直连目标库——验证库存在且可访问，失败时提示先创建数据库；
/// 3. 双阶段通过后写入配置文件并切换内存连接配置，前端据此进入
///    下一步（表创建流程）。
pub async fn test_db_connection(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<DatabaseSetupRequest>,
) -> Result<Response, InitError> {
    if !ctx.init_enabled() {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }
    req.validate()?;

    let cfg = req.into_database_config(&ctx.db_config());

    probe_database(&cfg, "postgres", "postgres").await?;
    probe_database(&cfg, &cfg.database, "target").await?;

    // 先落盘配置，再切换内存连接：落盘失败时内存保持旧值，
    // 避免「内存已切换、磁盘未更新」的不一致
    if let Err(e) = update_config_database(&ctx.config_path, &cfg).await {
        return Err(InitError::Internal(
            msg("server.init.db.config_write_failed").with("error", e),
        ));
    }
    ctx.set_db_config(cfg.clone());

    foims_common::log_info!(
        "log.init.db.test_passed",
        host = cfg.host,
        port = cfg.port,
        database = cfg.database
    );
    foims_common::log_info!("log.init.db.config_written");

    Ok(ok_json(
        serde_json::json!({ "connected": true }),
        "server.init.db.test.passed",
    ))
}

/// 创建数据库：以页面输入的凭据连接 postgres 系统库后创建目标库。
///
/// - 目标库已存在时幂等跳过（不视为错误，提示直接连接测试）；
/// - 实例上存在其他数据库不受任何影响（仅按库名精确匹配/创建）；
/// - 失败按分类反馈：服务器不可达 / 认证被拒 / 无 CREATEDB 权限等。
///
/// 建库成功后不直接进入下一流程：仍需通过「连接测试」按钮验证并
/// 写入配置（测试通过会写入配置文件并切换内存连接）。
pub async fn provision_database(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<DatabaseSetupRequest>,
) -> Result<Response, InitError> {
    if !ctx.init_enabled() {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }
    req.validate()?;

    let cfg = req.into_database_config(&ctx.db_config());

    let outcome = create_database(&cfg).await.map_err(|e| {
        // create_database 已按分类填充消息 key（连接失败/建库失败），
        // 此处以 Validation 透传 key 供前端精确提示
        foims_common::log_warn!("log.init.db.provision_failed", error = e);
        InitError::Validation(e)
    })?;

    match outcome {
        CreateOutcome::Created => {
            foims_common::log_info!("log.init.db.provision_created", name = cfg.database);
            Ok(ok_json(
                serde_json::json!({ "created": true }),
                "server.init.db.provision.created",
            ))
        }
        CreateOutcome::AlreadyExists => {
            foims_common::log_info!("log.init.db.provision_exists", name = cfg.database);
            Ok(ok_json(
                serde_json::json!({ "created": false }),
                "server.init.db.provision.exists",
            ))
        }
    }
}
