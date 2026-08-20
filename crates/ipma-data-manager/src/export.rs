//! 模块化 JSON 数据导出与导入模板下载。
//!
//! 每个业务模块导出为一个 JSON 文件（结构见 [`module_json`]），
//! `type=all` 时打包为 ZIP，单个模块直接下载 JSON 文件。
//! 设备表中的 SNMP 凭据字段在库内为密文，导出时解密为明文，
//! 以便跨实例导入（导入端会重新用本实例密钥加密）。

use crate::modules::{MODULES, ModuleDef};
use crate::types::{DataError, DataProvider, DataResult};
use axum::extract::Query;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use ipma_common::msg;
use serde_json::{Value, json};
use sqlx::AssertSqlSafe;
use sqlx::PgConnection;
use sqlx::types::Json;
use std::collections::HashMap;
use std::io::{Cursor, Write};
use zip::{ZipWriter, write::FileOptions};

/// 导出文件内的结构版本号：导入端据此识别格式演变。
const FORMAT_VERSION: u32 = 1;

/// devices 表中密文存储、导出时需解密的凭据列。
const DEVICE_SECRET_COLUMNS: &[&str] =
    &["snmp_community", "snmp_auth_password", "snmp_priv_password"];

/// 将多个文件打包为 ZIP 字节流。
fn zip_files(files: Vec<(String, Vec<u8>)>) -> DataResult<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    let options = FileOptions::<'_, ()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut zip = ZipWriter::new(&mut buf);
    for (filename, data) in files {
        zip.start_file(filename.as_str(), options).map_err(|e| {
            DataError::Internal(msg("server.import_export.zip_create_failed").with("error", e))
        })?;
        zip.write_all(&data).map_err(|e| {
            DataError::Internal(msg("server.import_export.zip_write_failed").with("error", e))
        })?;
    }
    zip.finish().map_err(|e| {
        DataError::Internal(msg("server.import_export.zip_finish_failed").with("error", e))
    })?;

    Ok(buf.into_inner())
}

/// 读取一张表的全部行（JSON 对象数组），按主键排序保证导出稳定。
async fn fetch_table_rows(conn: &mut PgConnection, table: &str) -> DataResult<Vec<Value>> {
    // table 来自模块静态白名单；AssertSqlSafe 表示该拼接已经人工审计无注入风险
    let sql = format!("SELECT to_jsonb(t) FROM {table} t ORDER BY id");
    let rows: Vec<Json<Value>> = sqlx::query_scalar(AssertSqlSafe(sql))
        .fetch_all(conn)
        .await
        .map_err(DataError::from)?;
    Ok(rows.into_iter().map(|j| j.0).collect())
}

/// 组装单个模块的导出 JSON。
fn module_json(module: &ModuleDef, tables: HashMap<&str, Vec<Value>>) -> Value {
    let tables_json: serde_json::Map<String, Value> = module
        .tables
        .iter()
        .map(|t| {
            let rows = tables.get(*t).cloned().unwrap_or_default();
            ((*t).to_string(), json!(rows))
        })
        .collect();
    json!({
        "module": module.name,
        "format_version": FORMAT_VERSION,
        "exported_at": Utc::now().to_rfc3339(),
        "tables": tables_json,
    })
}

/// 下载响应（JSON 单模块或 ZIP 全量）。
fn file_response(data: Vec<u8>, content_type: &str, filename: String) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        data,
    )
        .into_response()
}

/// 按模块导出业务数据为 JSON。
///
/// `type` 缺省为 `all`（全部模块打包 ZIP）；传入模块名时仅导出该模块（单个 JSON 文件）。
pub async fn export_json<P: DataProvider>(
    provider: P,
    type_param: Query<HashMap<String, String>>,
) -> DataResult<Response> {
    let export_type = type_param
        .get("type")
        .cloned()
        .unwrap_or_else(|| "all".to_string());

    let selected: Vec<&ModuleDef> = if export_type == "all" {
        MODULES.iter().collect()
    } else {
        vec![crate::modules::find_module(&export_type).ok_or_else(|| {
            DataError::Validation(
                msg("server.import_export.unknown_export_type").with("type", &export_type),
            )
        })?]
    };

    let pool = provider.pool()?;
    let mut conn = pool.acquire().await.map_err(DataError::from)?;
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");

    let mut module_payloads: Vec<(String, Vec<u8>)> = Vec::new();
    for module in selected {
        let mut tables: HashMap<&str, Vec<Value>> = HashMap::new();
        for table in module.tables {
            let mut rows = fetch_table_rows(&mut conn, table).await?;
            // 设备凭据列密文 → 明文，跨实例导入时由对方重新加密
            if *table == "devices" {
                for row in &mut rows {
                    decrypt_device_secrets(&provider, row).await?;
                }
            }
            tables.insert(table, rows);
        }
        let payload = serde_json::to_vec_pretty(&module_json(module, tables)).map_err(|e| {
            DataError::Internal(msg("server.import_export.json_build_failed").with("error", e))
        })?;
        module_payloads.push((format!("{}.json", module.name), payload));
    }

    if export_type == "all" {
        let zip_data = tokio::task::spawn_blocking(move || zip_files(module_payloads))
            .await
            .map_err(|e| {
                DataError::Internal(msg("server.import_export.zip_create_failed").with("error", e))
            })??;
        Ok(file_response(
            zip_data,
            "application/zip",
            format!("ipma_export_all_{timestamp}.zip"),
        ))
    } else {
        let (filename, payload) = module_payloads
            .into_iter()
            .next()
            .ok_or_else(|| DataError::Internal(msg("server.import_export.json_build_failed")))?;
        let stem = filename.trim_end_matches(".json");
        Ok(file_response(
            payload,
            "application/json",
            format!("ipma_export_{stem}_{timestamp}.json"),
        ))
    }
}

/// 解密设备行中的 SNMP 凭据字段（空值跳过）。
async fn decrypt_device_secrets<P: DataProvider>(provider: &P, row: &mut Value) -> DataResult<()> {
    let Some(obj) = row.as_object_mut() else {
        return Ok(());
    };
    for col in DEVICE_SECRET_COLUMNS {
        if let Some(Value::String(encrypted)) = obj.get(*col).cloned() {
            if encrypted.is_empty() {
                continue;
            }
            let plain = provider.decrypt_password(&encrypted).await?;
            obj.insert((*col).to_string(), Value::String(plain));
        }
    }
    Ok(())
}

/// 下载导入模板：全部模块的空结构 JSON，打包为 ZIP。
/// 不访问数据库，与导出结构完全一致，便于手工构造或对照修改。
pub async fn download_template(type_param: Query<HashMap<String, String>>) -> DataResult<Response> {
    let export_type = type_param
        .get("type")
        .cloned()
        .unwrap_or_else(|| "all".to_string());

    let selected: Vec<&ModuleDef> = if export_type == "all" {
        MODULES.iter().collect()
    } else {
        vec![crate::modules::find_module(&export_type).ok_or_else(|| {
            DataError::Validation(
                msg("server.import_export.unknown_export_type").with("type", &export_type),
            )
        })?]
    };

    let files: Vec<(String, Vec<u8>)> = selected
        .iter()
        .map(|module| {
            let payload =
                serde_json::to_vec_pretty(&module_json(module, HashMap::new())).map_err(|e| {
                    DataError::Internal(
                        msg("server.import_export.json_build_failed").with("error", e),
                    )
                })?;
            Ok((format!("{}.json", module.name), payload))
        })
        .collect::<DataResult<Vec<_>>>()?;

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let zip_data = zip_files(files)?;
    Ok(file_response(
        zip_data,
        "application/zip",
        format!("ipma_import_template_{timestamp}.zip"),
    ))
}
