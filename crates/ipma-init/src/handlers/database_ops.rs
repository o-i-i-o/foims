use std::sync::Arc;

use axum::extract::{Json, Multipart, State};
use axum::response::Response;
use sqlx::PgPool;
use std::path::PathBuf;
use tracing::{info, warn};

use crate::check::{check_has_data, validate_table_columns};
use crate::connection::ensure_database_and_schema;
use crate::context::InitContext;
use crate::error::{InitError, ok_json};
use crate::operations::{backup_database, create_database, drop_all_tables, drop_database};
use crate::schema::create_tables;
use crate::types::{CreateDatabaseRequest, CreateDatabaseResponse, ImportDatabaseRequest};
use crate::utils::PgPassFile;
use crate::verification::verify_code;

pub async fn create_database_api(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<CreateDatabaseRequest>,
) -> Result<Response, InitError> {
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
            ctx.db_config.username, ctx.db_config.password, ctx.db_config.host, ctx.db_config.port
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
    Ok(ok_json(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库创建成功".to_string(),
        },
        "数据库创建成功",
    ))
}

pub async fn import_database_api(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<ImportDatabaseRequest>,
) -> Result<Response, InitError> {
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
            ctx.db_config.username, ctx.db_config.password, ctx.db_config.host, ctx.db_config.port
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
    Ok(ok_json(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库导入成功".to_string(),
        },
        "数据库导入成功",
    ))
}

pub async fn import_database_from_file(
    State(ctx): State<Arc<InitContext>>,
    mut payload: Multipart,
) -> Result<Response, InitError> {
    if !ctx.init_enabled {
        return Err(InitError::Forbidden("系统初始化已在配置中禁用".to_string()));
    }

    let mut verification_code: Option<String> = None;
    let mut sql_file_path: Option<PathBuf> = None;

    tokio::fs::create_dir_all("/tmp/ipma_import")
        .await
        .map_err(|e| InitError::Internal(format!("创建临时目录失败: {e}")))?;

    const VERIFICATION_MAX: usize = 10 * 1024 * 1024;
    const SQL_FILE_MAX: usize = 100 * 1024 * 1024;

    while let Some(mut field) = payload
        .next_field()
        .await
        .map_err(|e| InitError::Validation(e.to_string()))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "verification" {
            let data = field
                .bytes()
                .await
                .map_err(|e| InitError::Validation(e.to_string()))?;
            if data.len() > VERIFICATION_MAX {
                return Err(InitError::Validation("验证码字段超过10MB限制".to_string()));
            }
            verification_code = Some(String::from_utf8_lossy(&data).to_string());
        } else if field_name == "sql_file" {
            let raw_filename = field
                .file_name()
                .map(std::string::ToString::to_string)
                .unwrap_or_else(|| "import.sql".to_string());
            let safe_name = raw_filename
                .split(['/', '\\'])
                .filter(|part| *part != ".." && *part != ".")
                .collect::<Vec<&str>>()
                .join("_");
            let safe_name = if safe_name.is_empty() {
                "import.sql".to_string()
            } else {
                safe_name
            };
            let filepath = PathBuf::from(format!("/tmp/ipma_import/{safe_name}"));

            let mut data = Vec::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| InitError::Validation(e.to_string()))?
            {
                data.extend_from_slice(&chunk);
                if data.len() > SQL_FILE_MAX {
                    return Err(InitError::Validation(
                        "SQL文件大小超过100MB限制".to_string(),
                    ));
                }
            }
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
            ctx.db_config.username, ctx.db_config.password, ctx.db_config.host, ctx.db_config.port
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
    Ok(ok_json(
        CreateDatabaseResponse {
            backup_file,
            message: "数据库导入成功".to_string(),
        },
        "数据库导入成功",
    ))
}

pub async fn clear_database(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<serde_json::Value>,
) -> Result<Response, InitError> {
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
    Ok(ok_json((), "数据库清空成功"))
}
