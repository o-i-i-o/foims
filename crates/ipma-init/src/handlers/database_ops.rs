//! 数据库级操作接口（创建/导入/备份/清空）。

use std::sync::Arc;

use axum::extract::{Json, Multipart, State};
use axum::response::Response;
use ipma_common::msg;
use sqlx::PgPool;
use std::path::PathBuf;
use uuid::Uuid;

use crate::check::{check_has_data, validate_table_columns};
use crate::connection::ensure_database_and_schema;
use crate::context::InitContext;
use crate::error::{InitError, ok_json};
use crate::operations::{backup_database, create_database, drop_all_tables, drop_database};
use crate::schema::create_tables;
use crate::types::{CreateDatabaseRequest, CreateDatabaseResponse};
use crate::utils::{PgPassFile, url_encode_component};
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

    // fail-fast：查询异常必须中止而非当作"无数据"跳过备份直接删库
    let has_data = check_has_data(&pool).await?;

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

        // 密码做 URL 编码后再拼连接串：含 @ : / 等字符时裸拼会导致连接失败（I-8）
        let postgres_url = format!(
            "postgres://{}:{}@{}:{}/postgres",
            url_encode_component(&ctx.db_config.username),
            url_encode_component(&ctx.db_config.password),
            ctx.db_config.host,
            ctx.db_config.port
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
    if !ctx.init_enabled() {
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

/// 上传 SQL 临时文件的清理守卫：无论处理成败（含提前返回错误），
/// 作用域结束时删除落盘文件，避免 /tmp/ipma_import 无限累积
struct TempSqlFile {
    path: PathBuf,
}

impl Drop for TempSqlFile {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            ipma_common::log_warn!("log.init.db.temp_file_remove_failed", error = e);
        }
    }
}

pub async fn import_database_from_file(
    State(ctx): State<Arc<InitContext>>,
    mut payload: Multipart,
) -> Result<Response, InitError> {
    if !ctx.init_enabled() {
        return Err(InitError::Forbidden(msg("server.init.disabled")));
    }

    let mut verification_code: Option<String> = None;
    // 守卫先占位：任何提前返回（校验失败/字段缺失）都不遗留临时文件
    let mut temp_file: Option<TempSqlFile> = None;

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
            // 落盘路径使用随机文件名 + create_new 原子创建（0600）：
            // 用户可控的可预测路径可能被本地低权用户以符号链接预置劫持（I-5）
            let filepath = PathBuf::from(format!("/tmp/ipma_import/import_{}.sql", Uuid::new_v4()));
            let mut opts = tokio::fs::OpenOptions::new();
            opts.mode(0o600).write(true).create_new(true);
            let mut file = opts.open(&filepath).await.map_err(|e| {
                InitError::Internal(msg("server.init.db.write_file_failed").with("error", e))
            })?;
            // 文件一旦创建立即交由守卫接管后续清理：write_all 失败的
            // 提前返回路径同样会删除残留的半成品文件
            temp_file = Some(TempSqlFile { path: filepath });
            use tokio::io::AsyncWriteExt;
            file.write_all(&data).await.map_err(|e| {
                InitError::Internal(msg("server.init.db.write_file_failed").with("error", e))
            })?;
        }
    }

    let Some(verification) = verification_code else {
        return Err(InitError::Validation(msg(
            "server.init.verification.missing",
        )));
    };

    let Some(temp) = temp_file else {
        return Err(InitError::Validation(msg(
            "server.init.db.sql_file_missing",
        )));
    };
    let sql_path = temp.path.clone();

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
            // 语句级失败立即中止并以非零码退出（psql 默认遇错继续且退出码为 0，
            // 半成品恢复会被误报成功）；--single-transaction 将整个脚本包在
            // 单个事务内，任一语句失败整体回滚，避免删库后留下残缺结构
            .arg("-v")
            .arg("ON_ERROR_STOP=1")
            .arg("--single-transaction")
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

    // 临时 SQL 文件由 TempSqlFile 守卫在本函数返回时删除（成功路径同样覆盖）
    drop(temp);

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
    if !ctx.init_enabled() {
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

    // 与其他重建路径（backup_and_drop_for_rebuild）口径一致：
    // 已有业务数据时先自动备份再清空，且查询失败必须中止而非当作
    // "无数据"跳过备份；备份失败同样中止，保留可恢复手段
    let has_data = check_has_data(&pool).await?;
    if has_data {
        ipma_common::log_info!("log.init.db.not_empty_backing_up");
        if let Err(e) = backup_database(&ctx.db_config).await {
            return Err(InitError::Internal(e));
        }
        ipma_common::log_info!("log.init.db.backup_done_dropping");
    }

    ipma_common::log_info!("log.init.db.clearing");
    if let Err(e) = drop_all_tables(&pool).await {
        return Err(InitError::Internal(
            msg("server.init.db.clear_failed").with("error", e),
        ));
    }

    ipma_common::log_info!("log.init.db.cleared");
    Ok(ok_json((), "server.init.db.cleared"))
}
