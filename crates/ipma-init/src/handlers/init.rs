//! 初始化执行接口（建库建表、初始管理员）。

use std::sync::Arc;

use axum::extract::{Json, State};
use axum::response::Response;
use ipma_common::msg;
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

    let count: i64 = match sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            if let Some(db_err) = e.as_database_error()
                && db_err.to_string().contains("UndefinedTable")
            {
                0
            } else {
                return Err(InitError::Database(
                    msg("server.init.db.query_failed").with("error", e),
                ));
            }
        }
    };

    if count > 0 {
        return Err(InitError::Validation(msg("server.init.db.has_user_data")));
    }

    if !check_required_tables_exist(&pool).await
        && let Err(e) = create_tables(&pool).await
    {
        return Err(InitError::Internal(
            msg("server.init.db.create_tables_failed").with("error", e),
        ));
    }

    let password_hash = hash_password(&req.password).await?;

    let user_id = Uuid::new_v4();

    ipma_common::log_info!(
        "log.init.creating_admin",
        id = user_id,
        username = req.username,
        email = req.email,
        role = req.role
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
        ipma_common::log_error!("log.init.admin_create_failed", error = e);
        return Err(InitError::Internal(
            msg("server.init.admin_create_failed").with("error", e),
        ));
    }

    if let Err(e) = update_config_enabled(&ctx.config_path, false).await {
        return Err(InitError::Internal(
            msg("server.init.config_update_failed").with("error", e),
        ));
    }

    ipma_common::log_info!("log.init.system_initialized", username = req.username);

    Ok(ok_json((), "server.init.completed"))
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
        ipma_common::log_info!("log.init.db.schema_exists");
    } else {
        if let Err(e) = create_tables(&pool).await {
            return Err(InitError::Internal(
                msg("server.init.db.create_tables_failed").with("error", e),
            ));
        }
        ipma_common::log_info!("log.init.db.schema_created");
    }

    Ok(ok_json((), "server.init.db.completed"))
}
