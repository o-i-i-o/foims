use actix_multipart::Multipart;
use actix_web::{HttpResponse, web};
use futures_util::TryStreamExt;
use sqlx::PgPool;
use std::path::PathBuf;
use tracing::{info, warn};
use uuid::Uuid;
use validator::Validate;

use crate::check::{check_has_data, check_required_tables_exist, validate_table_columns};
use crate::config::update_config_enabled;
use crate::connection::ensure_database_and_schema;
use crate::context::InitContext;
use crate::error::InitError;
use crate::operations::{backup_database, create_database, drop_all_tables, drop_database};
use crate::schema::create_tables;
use crate::types::{
    CreateDatabaseRequest, CreateDatabaseResponse, ImportDatabaseRequest, InitRequest,
};
use crate::utils::{PgPassFile, hash_password};
use crate::verification::verify_code;
use crate::ApiResponse;

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

pub async fn create_database_api(
    ctx: web::Data<InitContext>,
    req: web::Json<CreateDatabaseRequest>,
) -> Result<HttpResponse, InitError> {
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

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&ctx.db_config).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Err(InitError::Internal(format!("备份失败: {e}")));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&ctx.db_config).await {
            return Err(InitError::Internal(format!("删除数据库失败: {e}")));
        }
    } else {
        drop(pool);

        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            ctx.db_config.username,
            ctx.db_config.password,
            ctx.db_config.host,
            ctx.db_config.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Err(InitError::Internal(format!("连接PostgreSQL失败: {e}")));
            }
        };

        let db_exists: bool =
            match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&ctx.db_config.database)
                .fetch_one(&postgres_pool)
                .await
            {
                Ok(exists) => exists,
                Err(e) => {
                    return Err(InitError::Internal(format!("检查数据库失败: {e}")));
                }
            };

        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&ctx.db_config).await {
                return Err(InitError::Internal(format!("删除数据库失败: {e}")));
            }
        }
    }

    if let Err(e) = create_database(&ctx.db_config).await {
        return Err(InitError::Internal(format!("创建数据库失败: {e}")));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    if let Err(e) = create_tables(&pool).await {
        return Err(InitError::Internal(format!("创建表失败: {e}")));
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
    ctx: web::Data<InitContext>,
    req: web::Json<ImportDatabaseRequest>,
) -> Result<HttpResponse, InitError> {
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

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&ctx.db_config).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Err(InitError::Internal(format!("备份失败: {e}")));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&ctx.db_config).await {
            return Err(InitError::Internal(format!("删除数据库失败: {e}")));
        }
    } else {
        drop(pool);

        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            ctx.db_config.username,
            ctx.db_config.password,
            ctx.db_config.host,
            ctx.db_config.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Err(InitError::Internal(format!("连接PostgreSQL失败: {e}")));
            }
        };

        let db_exists: bool =
            match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&ctx.db_config.database)
                .fetch_one(&postgres_pool)
                .await
            {
                Ok(exists) => exists,
                Err(e) => {
                    return Err(InitError::Internal(format!("检查数据库失败: {e}")));
                }
            };

        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&ctx.db_config).await {
                return Err(InitError::Internal(format!("删除数据库失败: {e}")));
            }
        }
    }

    if let Err(e) = create_database(&ctx.db_config).await {
        return Err(InitError::Internal(format!("创建数据库失败: {e}")));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    if let Err(e) = create_tables(&pool).await {
        return Err(InitError::Internal(format!("创建表失败: {e}")));
    }

    if let Err(e) = validate_table_columns(&pool).await {
        return Err(InitError::Validation(format!(
            "数据库字段完整性校验失败: {e}"
        )));
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
    ctx: web::Data<InitContext>,
    mut payload: Multipart,
) -> Result<HttpResponse, InitError> {
    if !ctx.init_enabled {
        return Err(InitError::Forbidden("系统初始化已在配置中禁用".to_string()));
    }

    let mut verification_code: Option<String> = None;
    let mut sql_file_path: Option<PathBuf> = None;

    tokio::fs::create_dir_all("/tmp/ipma_import")
        .await
        .map_err(|e| InitError::Internal(format!("创建临时目录失败: {e}")))?;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|e| InitError::Validation(e.to_string()))?
    {
        let content_disposition = field.content_disposition();
        let field_name = content_disposition
            .map(|cd| cd.get_name().unwrap_or("").to_string())
            .unwrap_or_default();

        if field_name == "verification" {
            let data = field
                .bytes(10 * 1024 * 1024)
                .await
                .map_err(|e| InitError::Validation(e.to_string()))?
                .map_err(|e| InitError::Validation(e.to_string()))?;
            verification_code = Some(String::from_utf8_lossy(&data).to_string());
        } else if field_name == "sql_file" {
            let filename = content_disposition
                .and_then(|cd| cd.get_filename().map(std::string::ToString::to_string))
                .unwrap_or_else(|| "import.sql".to_string());
            let filepath = PathBuf::from(format!("/tmp/ipma_import/{filename}"));

            let data = field
                .bytes(100 * 1024 * 1024)
                .await
                .map_err(|e| InitError::Validation(e.to_string()))?
                .map_err(|e| InitError::Validation(e.to_string()))?;
            tokio::fs::write(&filepath, &data)
                .await
                .map_err(|e| InitError::Internal(format!("写入文件失败: {e}")))?;
            sql_file_path = Some(filepath);
        }
    }

    let Some(verification) = verification_code else {
        return Err(InitError::Validation("缺少验证码".to_string()));
    };

    let Some(sql_path) = sql_file_path else {
        return Err(InitError::Validation("缺少SQL文件".to_string()));
    };

    if let Err(e) = verify_code(&verification) {
        return Err(InitError::Validation(e));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    let has_data = check_has_data(&pool).await;
    let mut backup_file: Option<String> = None;

    if has_data {
        info!("数据库不为空，正在备份...");
        match backup_database(&ctx.db_config).await {
            Ok(file) => {
                backup_file = Some(file);
                info!("备份完成，正在删除旧数据库...");
            }
            Err(e) => {
                return Err(InitError::Internal(format!("备份失败: {e}")));
            }
        }

        drop(pool);

        if let Err(e) = drop_database(&ctx.db_config).await {
            return Err(InitError::Internal(format!("删除数据库失败: {e}")));
        }
    } else {
        drop(pool);

        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            ctx.db_config.username,
            ctx.db_config.password,
            ctx.db_config.host,
            ctx.db_config.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Err(InitError::Internal(format!("连接PostgreSQL失败: {e}")));
            }
        };

        let db_exists: bool =
            match sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(&ctx.db_config.database)
                .fetch_one(&postgres_pool)
                .await
            {
                Ok(exists) => exists,
                Err(e) => {
                    return Err(InitError::Internal(format!("检查数据库失败: {e}")));
                }
            };

        if db_exists {
            info!("数据库存在但无数据，正在删除重建...");
            if let Err(e) = drop_database(&ctx.db_config).await {
                return Err(InitError::Internal(format!("删除数据库失败: {e}")));
            }
        }
    }

    if let Err(e) = create_database(&ctx.db_config).await {
        return Err(InitError::Internal(format!("创建数据库失败: {e}")));
    }

    let db_config = ctx.db_config.clone();
    let sql_path_clone = sql_path.clone();
    let output = tokio::task::spawn_blocking(move || {
        let pgpass = PgPassFile::create(
            &db_config.host,
            db_config.port,
            &db_config.database,
            &db_config.username,
            &db_config.password,
        )
        .map_err(InitError::Internal)?;

        std::process::Command::new("psql")
            .arg("-h")
            .arg(&db_config.host)
            .arg("-p")
            .arg(db_config.port.to_string())
            .arg("-U")
            .arg(&db_config.username)
            .arg("-d")
            .arg(&db_config.database)
            .arg("-f")
            .arg(&sql_path_clone)
            .env("PGPASSFILE", pgpass.path())
            .output()
            .map_err(|e| InitError::Internal(format!("执行psql失败: {e}")))
    })
    .await
    .map_err(|e| InitError::Internal(format!("psql任务失败: {e}")))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InitError::Internal(format!("导入SQL文件失败: {stderr}")));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    if let Err(e) = validate_table_columns(&pool).await {
        return Err(InitError::Validation(format!(
            "数据库字段完整性校验失败: {e}"
        )));
    }

    if let Err(e) = tokio::fs::remove_file(&sql_path).await {
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
    ctx: web::Data<InitContext>,
    req: web::Json<InitRequest>,
) -> Result<HttpResponse, InitError> {
    (*req).validate()?;

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

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "系统初始化成功")))
}

pub async fn init_db(ctx: web::Data<InitContext>) -> Result<HttpResponse, InitError> {
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

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库初始化成功")))
}

pub async fn clear_database(
    ctx: web::Data<InitContext>,
    req: web::Json<serde_json::Value>,
) -> Result<HttpResponse, InitError> {
    if !ctx.init_enabled {
        return Err(InitError::Forbidden("系统初始化已在配置中禁用".to_string()));
    }

    let verification_code = match req.get("code") {
        Some(code) => code.as_str().unwrap_or(""),
        None => "",
    };

    if let Err(e) = verify_code(verification_code) {
        return Err(InitError::Validation(e));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    tracing::info!("正在通过API清空数据库...");
    if let Err(e) = drop_all_tables(&pool).await {
        return Err(InitError::Internal(format!("清空数据库失败: {e}")));
    }

    tracing::info!("通过API清空数据库成功");
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库清空成功")))
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
    Ok(HttpResponse::Ok().json(
        crate::ApiResponse::<()> {
            success: true,
            message: "服务重启命令已发送，服务正在重启...".to_string(),
            data: None,
        },
    ))
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
        ctx.db_config.username,
        ctx.db_config.password,
        ctx.db_config.host,
        ctx.db_config.port
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
