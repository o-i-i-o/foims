//! 初始化执行接口（建库建表、初始管理员）。

use std::sync::Arc;

use axum::extract::{Json, State};
use axum::response::Response;
use tracing::info;
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

pub async fn init_system(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<InitRequest>,
) -> Result<Response, InitError> {
    req.validate()?;

    if !ctx.init_enabled {
        return Err(InitError::Forbidden("系统初始化已在配置中禁用".to_string()));
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

    let count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            if let Some(db_err) = e.as_database_error() {
                if db_err.to_string().contains("UndefinedTable") {
                    0
                } else {
                    return Err(InitError::Database(format!("数据库查询错误: {e}")));
                }
            } else {
                return Err(InitError::Database(format!("数据库查询错误: {e}")));
            }
        }
    };

    if count > 0 {
        return Err(InitError::Validation(
            "数据库已有用户数据，请先通过新建或导入功能初始化数据库。".to_string(),
        ));
    }

    if !check_required_tables_exist(&pool).await
        && let Err(e) = create_tables(&pool).await
    {
        return Err(InitError::Internal(format!("创建表失败: {e}")));
    }

    let password_hash = hash_password(&req.password).await?;

    let user_id = Uuid::new_v4();

    tracing::info!(
        "正在创建管理员用户，ID: {}, 用户名: {}, 邮箱: {}, 角色: {}",
        user_id,
        req.username,
        req.email,
        req.role
    );

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
    .execute(&pool)
    .await
    {
        tracing::error!("创建管理员用户失败: {:?}", e);
        return Err(InitError::Database(format!("创建管理员用户失败: {e}")));
    }

    if let Err(e) = update_config_enabled(&ctx.config_path, false).await {
        return Err(InitError::Internal(format!("更新配置失败: {e}")));
    }

    info!(
        "系统初始化成功，管理员用户已创建: {}, 初始化模式已禁用",
        req.username
    );

    Ok(ok_json((), "系统初始化成功"))
}

pub async fn init_db(State(ctx): State<Arc<InitContext>>) -> Result<Response, InitError> {
    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    let required_tables_exist = check_required_tables_exist(&pool).await;

    if required_tables_exist {
        info!("数据库表结构已存在，跳过初始化");
    } else {
        if let Err(e) = create_tables(&pool).await {
            return Err(InitError::Internal(format!("创建表失败: {e}")));
        }
        info!("数据库表结构初始化成功");
    }

    Ok(ok_json((), "数据库初始化成功"))
}
