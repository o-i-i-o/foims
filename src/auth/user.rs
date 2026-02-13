use crate::config::Config;
use crate::db::DbPool;
use crate::models::{ApiResponse, User, UserCreate, UserUpdate};
use crate::utils::log_system_operation;
use actix_web::{HttpRequest, HttpResponse, Result, web};
use bcrypt::{DEFAULT_COST, hash};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use validator::Validate;

// 获取所有用户
pub async fn get_users(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let users = match sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users",
    )
    .fetch_all(pool.get_conn())
    .await
    {
        Ok(users) => users,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                format!("数据库查询错误: {}", err),
            )));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::<Vec<User>>::success(users, "用户获取成功")))
}

// 创建用户
pub async fn create_user(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    http_req: HttpRequest,
    req: web::Json<UserCreate>,
) -> Result<HttpResponse> {
    // 验证创建用户请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    // 检查用户名是否已存在
    let existing_user =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = $1")
            .bind(&req.username)
            .fetch_optional(pool.get_conn())
            .await
        {
            Ok(user) => user,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_user.is_some() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<User>::error("用户名已存在")));
    }

    // 检查邮箱是否已存在
    let existing_email =
        match sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE email = $1")
            .bind(&req.email)
            .fetch_optional(pool.get_conn())
            .await
        {
            Ok(email) => email,
            Err(err) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "Database query error: {}",
                        err
                    ))),
                );
            }
        };

    if existing_email.is_some() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<User>::error("邮箱已存在")));
    }

    // 密码哈希
    let hashed_password = match hash(&req.password, DEFAULT_COST) {
        Ok(password) => password,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("密码哈希错误: {}", err))));
        }
    };

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 创建用户
    if let Err(err) = sqlx::query(
        "INSERT INTO users (id, username, password_hash, email, role, status, created_at, updated_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(&req.username)
    .bind(&hashed_password)
    .bind(&req.email)
    .bind(&req.role)
    .bind(true)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                format!("数据库插入错误: {}", err),
            )));
    }

    // 记录操作日志
    let details = json!({"username": req.username, "email": req.email, "role": req.role});
    let _ = log_system_operation(
        &pool.pool,
        &http_req,
        &config,
        "create_user",
        "user",
        &id,
        &details,
        true,
    )
    .await;

    // 返回创建的用户
    let user = User {
        id,
        username: req.username.clone(),
        email: req.email.clone(),
        role: req.role.clone(),
        status: true,
        two_factor_enabled: false,
        two_factor_verified: false,
        created_at: now,
        updated_at: now,
    };

    Ok(HttpResponse::Ok().json(ApiResponse::<User>::success(user, "用户创建成功")))
}

// 获取单个用户
pub async fn get_user(pool: web::Data<DbPool>, id_path: web::Path<Uuid>) -> Result<HttpResponse> {
    let id = *id_path;

    let user = match sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    ).bind(id)
    .fetch_optional(pool.get_conn()).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<User>::error("用户未找到")));
        },
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::<User>::success(user, "用户获取成功")))
}

// 更新用户
pub async fn update_user(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    http_req: HttpRequest,
    id_path: web::Path<Uuid>,
    req: web::Json<UserUpdate>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 验证更新用户请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {:?}", e)))
        );
    }

    // 检查用户是否存在
    let existing_user = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(user) => user,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_user.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<User>::error("User not found")));
    }

    let now = Utc::now();

    // 更新用户信息
    if let Err(err) = sqlx::query(
        "UPDATE users SET 
         email = COALESCE($1, email), 
         role = COALESCE($2, role), 
         status = COALESCE($3, status), 
         updated_at = $4 
         WHERE id = $5",
    )
    .bind(&req.email)
    .bind(&req.role)
    .bind(req.status)
    .bind(now)
    .bind(id)
    .execute(pool.get_conn())
    .await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库更新错误: {}", err))));
    }

    // 记录操作日志
    let details = json!({"email": req.email, "role": req.role, "status": req.status});
    let _ = log_system_operation(
        &pool.pool,
        &http_req,
        &config,
        "update_user",
        "user",
        &id,
        &details,
        true,
    )
    .await;

    // 返回更新后的用户
    let user = match sqlx::query_as::<_, User>(
        "SELECT id, username, email, role, status, two_factor_enabled, two_factor_verified, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM users WHERE id = $1"
    ).bind(id)
    .fetch_one(pool.get_conn()).await {
        Ok(user) => user,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::<User>::success(user, "用户更新成功")))
}

// 删除用户
pub async fn delete_user(
    pool: web::Data<DbPool>,
    config: web::Data<Config>,
    http_req: HttpRequest,
    id_path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let id = *id_path;

    // 检查用户是否存在
    let existing_user = match sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool.get_conn())
        .await
    {
        Ok(user) => user,
        Err(err) => {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "Database query error: {}",
                    err
                ))),
            );
        }
    };

    if existing_user.is_none() {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("User not found")));
    }

    // 使用事务删除用户及其关联数据
    let mut tx = match pool.get_conn().begin().await {
        Ok(tx) => tx,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "数据库事务启动失败: {}",
                err
            ))));
        }
    };

    // 删除关联的操作日志
    if let Err(err) = sqlx::query("DELETE FROM operation_logs WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库删除错误: {}", err))));
    }

    // 删除关联的通知
    if let Err(err) = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库删除错误: {}", err))));
    }

    // 删除用户
    if let Err(err) = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
    {
        let _ = tx.rollback().await;
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("数据库删除错误: {}", err))));
    }

    // 提交事务
    if let Err(err) = tx.commit().await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("事务提交失败: {}", err))));
    }

    // 记录操作日志
    let details = json!({});
    let _ = log_system_operation(
        &pool.pool,
        &http_req,
        &config,
        "delete_user",
        "user",
        &id,
        &details,
        true,
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "用户删除成功")))
}
