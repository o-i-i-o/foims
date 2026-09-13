//! 数据库配置页接口（连接测试）。
//!
//! 供初始化向导第 2 步使用。端点只在 init 模式开放，且经路由层
//! localhost-only 中间件限制为本机访问；不要求控制台验证码——
//! 连接测试为只读探测（连通性 + 操作权限校验），无破坏性。
//!
//! 数据库与账号的创建不在向导内进行：由部署脚本
//! `scripts/init-pgsql.sh` 手动完成（建角色/建库/授权），页面仅
//! 负责验证与配置写入。
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
use crate::operations::classify_pg_connect_error;
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

/// 校验当前账号对目标库的操作权限：须为库所有者（或超级用户）且具有
/// CREATEDB 属性。
///
/// 背景：PostgreSQL 默认把新建库的 CONNECT 授予 PUBLIC，任意角色都能
/// 连接（登录）目标库，仅凭连通性探测会出现「任意用户名都能登录」；
/// 而第 3 步的新建/导入流程需要删库重建（DROP DATABASE 需所有权，
/// CREATE DATABASE 需 CREATEDB）。提前在此拦截「能登录但无权操作」
/// 的账号，避免到建表阶段才暴露权限问题。
async fn verify_operate_privilege(config: &DatabaseConfig) -> Result<(), InitError> {
    let url = build_pg_url(config, &config.database);
    let mut conn = PgConnection::connect(&url).await.map_err(|e| {
        let kind = classify_pg_connect_error(&e);
        foims_common::log_warn!(
            "log.init.db.test_failed",
            stage = "privilege",
            database = config.database,
            kind = kind.message_key(),
            error = e
        );
        InitError::Validation(msg(kind.message_key()))
    })?;

    // 第一列：库所有者或超级用户（DROP DATABASE 的必要条件，同时覆盖
    // public schema 建表权限——PG15+ 所有者经 pg_database_owner 隐式
    // 持有，PG14- 依赖 public schema 保留的 PUBLIC CREATE 授权，
    // 本次收权仅在库级别、不触及 schema 授权）；
    // 第二列：CREATEDB 属性（CREATE DATABASE 的必要条件）
    let check = sqlx::query_as::<_, (bool, bool)>(
        r#"
        SELECT pg_catalog.pg_get_userbyid(d.datdba) = current_user OR r.rolsuper,
               r.rolcreatedb OR r.rolsuper
        FROM pg_catalog.pg_database d
        JOIN pg_catalog.pg_roles r ON r.rolname = current_user
        WHERE d.datname = current_database()
        "#,
    )
    .fetch_one(&mut conn)
    .await;

    let (can_manage, can_create_db) = match check {
        Ok(v) => v,
        Err(e) => {
            if let Err(close_err) = conn.close().await {
                foims_common::log_warn!("log.init.db.probe_close_failed", error = close_err);
            }
            return Err(InitError::Internal(
                msg("server.init.db.check_failed").with("error", e),
            ));
        }
    };
    if let Err(e) = conn.close().await {
        foims_common::log_warn!("log.init.db.probe_close_failed", error = e);
    }

    if !can_manage || !can_create_db {
        foims_common::log_warn!(
            "log.init.db.test_no_privilege",
            database = config.database,
            can_manage = can_manage,
            can_create_db = can_create_db
        );
        return Err(InitError::Validation(msg(
            "server.init.db.test.no_privilege",
        )));
    }
    Ok(())
}

/// 连接测试（充当下一步按钮）：
///
/// 1. 连接 postgres 系统库——验证服务器可达与凭据，失败时区分
///    「服务器不可达」（检查地址/端口/防火墙）与「认证被拒」
///    （用户不存在或密码错误）；
/// 2. 直连目标库——验证库存在且可访问，失败时提示先用部署脚本
///    创建数据库与账号；
/// 3. 校验目标库操作权限——账号须为库所有者（或超级用户）且具有
///    CREATEDB，防止「能登录但无权操作」的账号进入后续流程；
/// 4. 全部通过后写入配置文件并切换内存连接配置，前端据此进入
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
    verify_operate_privilege(&cfg).await?;

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
