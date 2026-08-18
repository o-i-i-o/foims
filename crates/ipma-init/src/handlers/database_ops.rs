//! 数据库级操作接口（创建/导入/备份/清空）。

use std::sync::Arc;

use axum::extract::{Json, Multipart, State};
use axum::response::Response;
use ipma_common::msg;
use sqlx::PgPool;
use std::path::PathBuf;

use crate::check::{check_has_data, validate_table_columns};
use crate::connection::ensure_database_and_schema;
use crate::context::InitContext;
use crate::error::{InitError, ok_json};
use crate::operations::{backup_database, create_database, drop_all_tables, drop_database};
use crate::schema::create_tables;
use crate::types::{CreateDatabaseRequest, CreateDatabaseResponse, ImportDatabaseRequest};
use crate::utils::PgPassFile;
use crate::verification::verify_code;

/// 备份并删除现有数据库（有数据时先备份），为重建做准备。
///
/// 返回备份文件路径（未发生备份时为 `None`）。
async fn backup_and_drop_for_rebuild(ctx: &InitContext) -> Result<Option<String>, InitError> {
    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    let has_data = check_has_data(&pool).await;

    if has_data {
        ipma_common::log_info!("log.init.db.not_empty_backing_up");
        let backup_file = match backup_database(&ctx.db_config).await {
            Ok(file) => {
                ipma_common::log_info!("log.init.db.backup_done_dropping");
                Some(file)
            }
            Err(e) => {
                return Err(InitError::Internal(e));
            }
        };

        drop(pool);

        if let Err(e) = drop_database(&ctx.db_config).await {
            return Err(InitError::Internal(e));
        }

        Ok(backup_file)
    } else {
        drop(pool);

        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            ctx.db_config.username, ctx.db_config.password, ctx.db_config.host, ctx.db_config.port
        );
        let postgres_pool = match PgPool::connect(&postgres_url).await {
            Ok(p) => p,
            Err(e) => {
                return Err(InitError::Internal(
                    msg("server.init.db.pgsql_connect_failed").with("error", e),
                ));
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
                    return Err(InitError::Internal(
                        msg("server.init.db.check_failed").with("error", e),
                    ));
                }
            };

        if db_exists {
            ipma_common::log_info!("log.init.db.empty_db_recreating");
            if let Err(e) = drop_database(&ctx.db_config).await {
                return Err(InitError::Internal(e));
            }
        }

        Ok(None)
    }
}

/// 重建数据库并建表（备份与删库由 [`backup_and_drop_for_rebuild`] 完成），
/// 返回指向新库的连接池。
async fn recreate_database_with_tables(ctx: &InitContext) -> Result<PgPool, InitError> {
    if let Err(e) = create_database(&ctx.db_config).await {
        return Err(InitError::Internal(e));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    if let Err(e) = create_tables(&pool).await {
        return Err(InitError::Internal(
            msg("server.init.db.create_tables_failed").with("error", e),
        ));
    }

    Ok(pool)
}

pub async fn create_database_api(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<CreateDatabaseRequest>,
) -> Result<Response, InitError> {
    if !ctx.init_enabled {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }

    if let Err(e) = verify_code(&req.verification) {
        return Err(InitError::Validation(e));
    }

    let backup_file = backup_and_drop_for_rebuild(&ctx).await?;

    recreate_database_with_tables(&ctx).await?;

    ipma_common::log_info!("log.init.db.database_created");
    Ok(ok_json(
        CreateDatabaseResponse {
            backup_file,
            message: "server.init.db.created".to_string(),
        },
        "server.init.db.created",
    ))
}

pub async fn import_database_api(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<ImportDatabaseRequest>,
) -> Result<Response, InitError> {
    if !ctx.init_enabled {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }

    if let Err(e) = verify_code(&req.verification) {
        return Err(InitError::Validation(e));
    }

    let backup_file = backup_and_drop_for_rebuild(&ctx).await?;

    let pool = recreate_database_with_tables(&ctx).await?;

    if let Err(e) = validate_table_columns(&pool).await {
        return Err(InitError::Validation(e));
    }

    ipma_common::log_info!("log.init.db.imported");
    Ok(ok_json(
        CreateDatabaseResponse {
            backup_file,
            message: "server.init.db.imported".to_string(),
        },
        "server.init.db.imported",
    ))
}

pub async fn import_database_from_file(
    State(ctx): State<Arc<InitContext>>,
    mut payload: Multipart,
) -> Result<Response, InitError> {
    if !ctx.init_enabled {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }

    let mut verification_code: Option<String> = None;
    let mut sql_file_path: Option<PathBuf> = None;

    tokio::fs::create_dir_all("/tmp/ipma_import")
        .await
        .map_err(|e| {
            InitError::Internal(msg("server.init.db.temp_dir_create_failed").with("error", e))
        })?;

    const VERIFICATION_MAX: usize = 10 * 1024 * 1024;
    const SQL_FILE_MAX: usize = 100 * 1024 * 1024;

    while let Some(mut field) = payload.next_field().await.map_err(|e| {
        InitError::Validation(msg("server.init.db.file_read_failed").with("error", e))
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "verification" {
            let data = field.bytes().await.map_err(|e| {
                InitError::Validation(msg("server.init.db.file_read_failed").with("error", e))
            })?;
            if data.len() > VERIFICATION_MAX {
                return Err(InitError::Validation(msg(
                    "server.init.db.verification_field_too_large",
                )));
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
            while let Some(chunk) = field.chunk().await.map_err(|e| {
                InitError::Validation(msg("server.init.db.file_read_failed").with("error", e))
            })? {
                data.extend_from_slice(&chunk);
                if data.len() > SQL_FILE_MAX {
                    return Err(InitError::Validation(msg(
                        "server.init.db.sql_file_too_large",
                    )));
                }
            }
            tokio::fs::write(&filepath, &data).await.map_err(|e| {
                InitError::Internal(msg("server.init.db.write_file_failed").with("error", e))
            })?;
            sql_file_path = Some(filepath);
        }
    }

    let Some(verification) = verification_code else {
        return Err(InitError::Validation(msg(
            "server.init.verification.missing",
        )));
    };

    let Some(sql_path) = sql_file_path else {
        return Err(InitError::Validation(msg(
            "server.init.db.sql_file_missing",
        )));
    };

    if let Err(e) = verify_code(&verification) {
        return Err(InitError::Validation(e));
    }

    let backup_file = backup_and_drop_for_rebuild(&ctx).await?;

    if let Err(e) = create_database(&ctx.db_config).await {
        return Err(InitError::Internal(e));
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
            .map_err(|e| {
                InitError::Internal(msg("server.init.db.psql_exec_failed").with("error", e))
            })
    })
    .await
    .map_err(|e| InitError::Internal(msg("server.init.db.psql_task_failed").with("error", e)))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InitError::Internal(
            msg("server.init.db.sql_import_failed").with("error", stderr),
        ));
    }

    let pool = match ensure_database_and_schema(&ctx.db_config).await {
        Ok(p) => p,
        Err(e) => {
            return Err(InitError::Internal(e));
        }
    };

    if let Err(e) = validate_table_columns(&pool).await {
        return Err(InitError::Validation(e));
    }

    if let Err(e) = tokio::fs::remove_file(&sql_path).await {
        ipma_common::log_warn!("log.init.db.temp_file_remove_failed", error = e);
    }

    ipma_common::log_info!("log.init.db.imported_from_file");
    Ok(ok_json(
        CreateDatabaseResponse {
            backup_file,
            message: "server.init.db.imported".to_string(),
        },
        "server.init.db.imported",
    ))
}

pub async fn clear_database(
    State(ctx): State<Arc<InitContext>>,
    Json(req): Json<serde_json::Value>,
) -> Result<Response, InitError> {
    if !ctx.init_enabled {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
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

    ipma_common::log_info!("log.init.db.clearing");
    if let Err(e) = drop_all_tables(&pool).await {
        return Err(InitError::Internal(
            msg("server.init.db.clear_failed").with("error", e),
        ));
    }

    ipma_common::log_info!("log.init.db.cleared");
    Ok(ok_json((), "server.init.db.cleared"))
}
