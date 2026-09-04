//! 初始化执行接口（建库建表、初始管理员）。

use std::sync::Arc;

use axum::extract::{Json, State};
use axum::response::Response;
use foims_common::msg;
use uuid::Uuid;
use validator::Validate;

use crate::check::check_required_tables_exist;
use crate::config::update_config_enabled;
use crate::connection::ensure_database_and_schema;
use crate::context::InitContext;
use crate::error::{InitError, ok_json};
use crate::schema::create_tables;
use crate::types::InitRequest;
use crate::utils::hash_password;
use crate::verification::verify_code;

/// 初始化建户专用 advisory lock key（"FOIMS" 魔数 + 序号 1）
const INIT_ADMIN_LOCK_KEY: i64 = 0x464F_494D_5300_0001;

pub async fn init_system(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<InitRequest>,
) -> Result<Response, InitError> {
    req.validate()?;

    if !ctx.init_enabled() {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }

    if let Err(e) = verify_code(&req.verification) {
        return Err(InitError::Validation(e));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    if !check_required_tables_exist(&pool).await
        && let Err(e) = create_tables(&pool).await
    {
        return Err(InitError::Internal(
            msg("server.init.db.create_tables_failed").with("error", e),
        ));
    }

    let password_hash = hash_password(&req.password).await?;
    let user_id = Uuid::new_v4();

    foims_common::log_info!(
        "log.init.creating_admin",
        id = user_id,
        username = req.username,
        email = req.email,
        role = req.role
    );

    // 建户检查与插入放进同一事务并持有 advisory lock：
    // 否则并发调用可在 COUNT 检查后各自插入，创建出多个管理员（I-4）
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return Err(InitError::Database(
                msg("server.init.db.query_failed").with("error", e),
            ));
        }
    };

    // pg_advisory_xact_lock 返回 VOID，须用 execute 丢弃输出；
    // 按 i64 标量解码会因类型不匹配失败（与 user.rs 的锁获取方式一致）
    if let Err(e) = sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(INIT_ADMIN_LOCK_KEY)
        .execute(&mut *tx)
        .await
    {
        return Err(InitError::Database(
            msg("server.init.db.query_failed").with("error", e),
        ));
    }

    let count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&mut *tx)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            return Err(InitError::Database(
                msg("server.init.db.query_failed").with("error", e),
            ));
        }
    };

    if count > 0 {
        return Err(InitError::Validation(msg("server.init.db.has_user_data")));
    }

    if let Err(e) = sqlx::query(
        r"INSERT INTO users (id, username, password_hash, email, role, status)
               VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(&req.username)
    .bind(&password_hash)
    .bind(&req.email)
    .bind(&req.role)
    .bind(true)
    .execute(&mut *tx)
    .await
    {
        foims_common::log_error!("log.init.admin_create_failed", error = e);
        return Err(InitError::Internal(
            msg("server.init.admin_create_failed").with("error", e),
        ));
    }

    if let Err(e) = tx.commit().await {
        return Err(InitError::Database(
            msg("server.init.db.query_failed").with("error", e),
        ));
    }

    // 先落盘配置，再同步翻转内存开关：init 完成到重启之间不再存在
    // 「配置已写 init.enabled=false 但进程仍按 true 服务」的毁库窗口（I-3）
    if let Err(e) = update_config_enabled(&ctx.config_path, false).await {
        return Err(InitError::Internal(
            msg("server.init.config_update_failed").with("error", e),
        ));
    }
    ctx.disable_init();
    // 武装一次性重启许可：重启端点仅对刚完成初始化的请求放行一次
    ctx.arm_restart();

    foims_common::log_info!("log.init.system_initialized", username = req.username);

    Ok(ok_json((), "server.init.completed"))
}

pub async fn init_db(State(ctx): State<Arc<InitContext>>) -> Result<Response, InitError> {
    // 建表同样是危险操作：与其他 init 接口一致校验开关
    //（验证码经 init_system 消费后不可复用，此处不重复校验）
    if !ctx.init_enabled() {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    let required_tables_exist = check_required_tables_exist(&pool).await;

    if required_tables_exist {
        foims_common::log_info!("log.init.db.schema_exists");
    } else {
        if let Err(e) = create_tables(&pool).await {
            return Err(InitError::Internal(
                msg("server.init.db.create_tables_failed").with("error", e),
            ));
        }
        foims_common::log_info!("log.init.db.schema_created");
    }

    Ok(ok_json((), "server.init.db.completed"))
}
