use actix_multipart::Multipart;
use actix_web::{HttpResponse, Result, web};
use bcrypt::hash;
use futures_util::TryStreamExt;
use sqlx::PgPool;
use std::io::Write;
use std::path::PathBuf;
use tracing::{info, warn};
use uuid::Uuid;
use validator::Validate;

use crate::config::Config;
use crate::init::check::{check_has_data, check_required_tables_exist, validate_table_columns};
use crate::init::config::update_config_enabled;
use crate::init::connection::ensure_database_and_schema;
use crate::init::operations::{backup_database, create_database, drop_all_tables, drop_database};
use crate::init::schema::create_tables;
use crate::init::types::{
    BCRYPT_COST, CreateDatabaseRequest, CreateDatabaseResponse, ImportDatabaseRequest, InitRequest,
};
use crate::init::verification::verify_code;
use crate::models::ApiResponse;

pub async fn check_db_status(config: web::Data<Config>) -> Result<HttpResponse> {
    let pool = match ensure_database_and_schema(&config.database).await {
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

pub async fn create_database_api(
    config: web::Data<Config>,
    req: web::Json<CreateDatabaseRequest>,
) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    if let Err(e) = verify_code(&req.verification) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
        }
    };

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&config.database).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("备份失败: {e}"))));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&config.database).await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除数据库失败: {e}"))));
        }
    } else {
        drop(pool);

        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            config.database.username,
            config.database.password,
            config.database.host,
            config.database.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "连接PostgreSQL失败: {e}"
                    ))),
                );
            }
        };

        let db_exists: bool =
            match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&config.database.database)
                .fetch_one(&postgres_pool)
                .await
            {
                Ok(exists) => exists,
                Err(e) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("检查数据库失败: {e}"))));
                }
            };

        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&config.database).await {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("删除数据库失败: {e}"))));
            }
        }
    }

    if let Err(e) = create_database(&config.database).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建数据库失败: {e}"))));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
        }
    };

    if let Err(e) = create_tables(&pool).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建表失败: {e}"))));
    }

    info!("数据库创建成功");
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库创建成功".to_string(),
        },
        "数据库创建成功",
    )))
}

pub async fn import_database_api(
    config: web::Data<Config>,
    req: web::Json<ImportDatabaseRequest>,
) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    if let Err(e) = verify_code(&req.verification) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
        }
    };

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&config.database).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("备份失败: {e}"))));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&config.database).await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除数据库失败: {e}"))));
        }
    } else {
        drop(pool);

        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            config.database.username,
            config.database.password,
            config.database.host,
            config.database.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "连接PostgreSQL失败: {e}"
                    ))),
                );
            }
        };

        let db_exists: bool =
            match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&config.database.database)
                .fetch_one(&postgres_pool)
                .await
            {
                Ok(exists) => exists,
                Err(e) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("检查数据库失败: {e}"))));
                }
            };

        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&config.database).await {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("删除数据库失败: {e}"))));
            }
        }
    }

    if let Err(e) = create_database(&config.database).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建数据库失败: {e}"))));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
        }
    };

    if let Err(e) = create_tables(&pool).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建表失败: {e}"))));
    }

    if let Err(e) = validate_table_columns(&pool).await {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "数据库字段完整性校验失败: {e}"
            ))),
        );
    }

    info!("数据库导入成功");
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库导入成功".to_string(),
        },
        "数据库导入成功",
    )))
}

pub async fn import_database_from_file(
    config: web::Data<Config>,
    mut payload: Multipart,
) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    let mut verification_code: Option<String> = None;
    let mut sql_file_path: Option<PathBuf> = None;

    std::fs::create_dir_all("/tmp/ipma_import").map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("创建临时目录失败: {e}"))
    })?;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(actix_web::error::ErrorBadRequest)?
    {
        let content_disposition = field.content_disposition();
        let field_name = content_disposition
            .map(|cd| cd.get_name().unwrap_or("").to_string())
            .unwrap_or_default();

        if field_name == "verification" {
            let data = field
                .bytes(10 * 1024 * 1024)
                .await
                .map_err(actix_web::error::ErrorBadRequest)?
                .map_err(actix_web::error::ErrorBadRequest)?;
            verification_code = Some(String::from_utf8_lossy(&data).to_string());
        } else if field_name == "sql_file" {
            let filename = content_disposition
                .and_then(|cd| cd.get_filename().map(std::string::ToString::to_string))
                .unwrap_or_else(|| "import.sql".to_string());
            let filepath = PathBuf::from(format!("/tmp/ipma_import/{filename}"));
            let mut f = std::fs::File::create(&filepath).map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!("创建文件失败: {e}"))
            })?;

            let data = field
                .bytes(100 * 1024 * 1024)
                .await
                .map_err(actix_web::error::ErrorBadRequest)?
                .map_err(actix_web::error::ErrorBadRequest)?;
            f.write_all(&data).map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!("写入文件失败: {e}"))
            })?;
            sql_file_path = Some(filepath);
        }
    }

    let Some(verification) = verification_code else {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少验证码")))
    };

    let Some(sql_path) = sql_file_path else {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少SQL文件")))
    };

    if let Err(e) = verify_code(&verification) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
        }
    };

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&config.database).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("备份失败: {e}"))));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&config.database).await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("删除数据库失败: {e}"))));
        }
    } else {
        drop(pool);

        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            config.database.username,
            config.database.password,
            config.database.host,
            config.database.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "连接PostgreSQL失败: {e}"
                    ))),
                );
            }
        };

        let db_exists: bool =
            match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&config.database.database)
                .fetch_one(&postgres_pool)
                .await
            {
                Ok(exists) => exists,
                Err(e) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("检查数据库失败: {e}"))));
                }
            };

        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&config.database).await {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("删除数据库失败: {e}"))));
            }
        }
    }

    if let Err(e) = create_database(&config.database).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建数据库失败: {e}"))));
    }

    let output = std::process::Command::new("psql")
        .arg("-h")
        .arg(&config.database.host)
        .arg("-p")
        .arg(config.database.port.to_string())
        .arg("-U")
        .arg(&config.database.username)
        .arg("-d")
        .arg(&config.database.database)
        .arg("-f")
        .arg(&sql_path)
        .env("PGPASSWORD", &config.database.password)
        .output()
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("执行psql失败: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "导入SQL文件失败: {stderr}"
            ))),
        );
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
        }
    };

    if let Err(e) = validate_table_columns(&pool).await {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "数据库字段完整性校验失败: {e}"
            ))),
        );
    }

    if let Err(e) = std::fs::remove_file(&sql_path) {
        warn!("删除SQL临时文件失败: {}", e);
    }

    info!("数据库从文件导入成功");
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库导入成功".to_string(),
        },
        "数据库导入成功",
    )))
}

pub async fn init_system(
    config: web::Data<Config>,
    req: web::Json<InitRequest>,
) -> Result<HttpResponse> {
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    if let Err(e) = verify_code(&req.verification) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
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
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("数据库查询错误: {e}"))));
                }
            } else {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("数据库查询错误: {e}"))));
            }
        }
    };

    if count > 0 {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "数据库已有用户数据，请先通过新建或导入功能初始化数据库。",
        )));
    }

    if !check_required_tables_exist(&pool).await
        && let Err(e) = create_tables(&pool).await
    {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建表失败: {e}"))));
    }

    let password_hash = match hash(&req.password, BCRYPT_COST) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("密码哈希错误: {:?}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("密码哈希错误: {e}"))));
        }
    };

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
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "创建管理员用户失败: {e}"
            ))),
        );
    }

    if let Err(e) = update_config_enabled(false) {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("更新配置失败: {e}"))));
    }

    info!(
        "系统初始化成功，管理员用户已创建: {}, 初始化模式已禁用",
        req.username
    );

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "系统初始化成功")))
}

pub async fn init_db(config: web::Data<Config>) -> Result<HttpResponse> {
    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
        }
    };

    let required_tables_exist = check_required_tables_exist(&pool).await;

    if required_tables_exist {
        info!("数据库表结构已存在，跳过初始化");
    } else {
        if let Err(e) = create_tables(&pool).await {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("创建表失败: {e}"))));
        }
        info!("数据库表结构初始化成功");
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库初始化成功")))
}

pub async fn clear_database(
    config: web::Data<Config>,
    req: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    if !config.init.enabled {
        return Ok(
            HttpResponse::Forbidden().json(ApiResponse::<()>::error("系统初始化已在配置中禁用"))
        );
    }

    let verification_code = match req.get("code") {
        Some(code) => code.as_str().unwrap_or(""),
        None => "",
    };

    if let Err(e) = verify_code(verification_code) {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(e)));
    }

    let pool = match ensure_database_and_schema(&config.database).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(e)));
        }
    };

    tracing::info!("正在通过API清空数据库...");
    if let Err(e) = drop_all_tables(&pool).await {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("清空数据库失败: {e}"))));
    }

    tracing::info!("通过API清空数据库成功");
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库清空成功")))
}

pub async fn check_init_status(config: web::Data<Config>) -> Result<HttpResponse> {
    let Ok(pool) = ensure_database_and_schema(&config.database).await else {
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "initialized": false,
            "version": env!("CARGO_PKG_VERSION"),
        })))
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

pub async fn restart_program() -> Result<HttpResponse> {
    info!("收到重启程序请求，正在准备重启...");
    crate::system::config::trigger_service_restart()
}

pub async fn check_pgsql(config: web::Data<Config>) -> Result<HttpResponse> {
    let installed = std::process::Command::new("which")
        .arg("psql")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !installed {
        return Ok(HttpResponse::Ok().json(serde_json::json!({
            "installed": false,
            "running": false,
            "error": "PostgreSQL is not installed. Please install PostgreSQL first."
        })));
    }

    let url = format!(
        "postgres://{}:{}@{}:{}/postgres",
        config.database.username,
        config.database.password,
        config.database.host,
        config.database.port
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
