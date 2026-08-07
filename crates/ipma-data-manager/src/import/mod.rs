use crate::types::{DataError, DataProvider, DataResult, ok_json};
use axum::extract::{Multipart, Query};
use axum::response::Response;
use serde_json::json;
use sqlx::Acquire;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Arc;

pub mod cabinets;
pub mod network_regions;
pub mod networks;
pub mod positions;
pub mod rooms;
pub mod switches;
pub mod workstations;

pub(crate) fn empty_to_none(s: &str) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s.trim().to_string())
    }
}

pub(crate) async fn find_network_id(
    conn: &mut sqlx::PgConnection,
    network_identifier: &str,
) -> DataResult<Option<uuid::Uuid>> {
    if network_identifier.contains('/') {
        let parts: Vec<&str> = network_identifier.splitn(2, '/').collect();
        if parts.len() == 2 {
            let region_name = parts[0].trim();
            let network_name = parts[1].trim();
            sqlx::query_scalar(
                "SELECT n.id FROM network_cidrs n 
                 JOIN network_regions r ON n.network_region_id = r.id 
                 WHERE r.name = $1 AND n.name = $2",
            )
            .bind(region_name)
            .bind(network_name)
            .fetch_optional(&mut *conn)
            .await
            .map_err(DataError::from)
        } else {
            Ok(None)
        }
    } else {
        sqlx::query_scalar("SELECT id FROM network_cidrs WHERE name = $1")
            .bind(network_identifier.trim())
            .fetch_optional(&mut *conn)
            .await
            .map_err(DataError::from)
    }
}

pub async fn import_csv<P: DataProvider>(
    provider: P,
    mut payload: Multipart,
    query: Query<HashMap<String, String>>,
) -> DataResult<Response> {
    let mode = query
        .get("mode")
        .cloned()
        .unwrap_or_else(|| "skip".to_string());
    let overwrite = mode == "overwrite";

    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;

    while let Some(mut field) = payload
        .next_field()
        .await
        .map_err(|e| DataError::Internal(format!("读取文件失败: {e}")))?
    {
        if field.name() == Some("file") {
            filename = field.file_name().map(std::string::ToString::to_string);
            let mut data = Vec::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| DataError::Internal(format!("读取文件块失败: {e}")))?
            {
                data.extend_from_slice(&chunk);
                if data.len() > MAX_UPLOAD_SIZE {
                    return Err(DataError::Validation("文件大小超过50MB限制".to_string()));
                }
            }
            file_data = Some(data);
            break;
        }
    }

    let file_data =
        file_data.ok_or_else(|| DataError::Validation("请选择要导入的CSV文件".to_string()))?;
    let file_data = Arc::new(file_data);

    let pool = provider.pool()?;
    let mut conn = pool.acquire().await.map_err(DataError::from)?;

    let mut results = Vec::new();
    let file_data_clone = file_data.clone();
    let filename_clone = filename.clone();

    let csv_entries = tokio::task::spawn_blocking(move || {
        const MAX_DECOMPRESSED_SIZE: u64 = 100 * 1024 * 1024;
        const MAX_TOTAL_DECOMPRESSED: u64 = 500 * 1024 * 1024;
        let mut total_decompressed: u64 = 0;
        let mut entries = Vec::new();
        if let Ok(mut zip) = zip::ZipArchive::new(Cursor::new((*file_data_clone).clone())) {
            for i in 0..zip.len() {
                let mut file = zip
                    .by_index(i)
                    .map_err(|e| DataError::Internal(format!("读取ZIP文件项失败: {e}")))?;

                let zip_filename = file.name().to_string();
                if zip_filename.contains("..")
                    || zip_filename.contains('/')
                    || zip_filename.contains('\\')
                {
                    continue;
                }
                if zip_filename.ends_with(".csv") {
                    let mut limited = (&mut file).take(MAX_DECOMPRESSED_SIZE);
                    let mut content = String::new();
                    limited
                        .read_to_string(&mut content)
                        .map_err(|e| DataError::Internal(format!("读取CSV文件失败: {e}")))?;
                    let entry_size = content.len() as u64;
                    total_decompressed = total_decompressed.saturating_add(entry_size);
                    if total_decompressed > MAX_TOTAL_DECOMPRESSED {
                        return Err(DataError::Internal(format!(
                            "ZIP解压总大小超过限制({}MB),可能为ZIP炸弹",
                            MAX_TOTAL_DECOMPRESSED / 1024 / 1024
                        )));
                    }
                    entries.push((zip_filename.trim_end_matches(".csv").to_string(), content));
                }
            }
        } else {
            let content = String::from_utf8(file_data_clone.as_ref().clone()).map_err(|e| {
                DataError::Validation(format!("解析CSV文件失败: 文件编码必须是UTF-8 - {e}"))
            })?;
            let table_name = filename_clone
                .as_ref()
                .and_then(|f| f.trim_end_matches(".csv").split('.').next())
                .unwrap_or("unknown")
                .to_string();
            entries.push((table_name, content));
        }
        Ok::<Vec<(String, String)>, DataError>(entries)
    })
    .await
    .map_err(|e| DataError::Internal(format!("ZIP解压任务失败: {e}")))??;

    for (table_name, content) in csv_entries {
        let mut tx = conn.begin().await.map_err(DataError::from)?;
        match process_csv_by_filename(&mut tx, &table_name, &content, overwrite, &mut results).await
        {
            Ok(()) => {
                if let Err(e) = tx.commit().await {
                    results.push(format!("提交 {table_name}.csv 事务失败: {e}"));
                }
            }
            Err(e) => {
                results.push(format!("导入 {table_name}.csv 失败: {e}"));
                if let Err(rb_err) = tx.rollback().await {
                    tracing::warn!("回滚 {table_name}.csv 事务失败: {rb_err}");
                }
            }
        }
    }

    Ok(ok_json(json!({ "results": results }), "导入完成"))
}

async fn process_csv_by_filename(
    conn: &mut sqlx::PgConnection,
    filename: &str,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> DataResult<()> {
    match filename {
        "network_regions" => {
            network_regions::import_network_regions(conn, content, overwrite, results).await
        }
        "networks" => networks::import_networks(conn, content, overwrite, results).await,
        "rooms" => rooms::import_rooms(conn, content, overwrite, results).await,
        "workstations" => {
            workstations::import_workstations(conn, content, overwrite, results).await
        }
        "cabinets" => cabinets::import_cabinets(conn, content, overwrite, results).await,
        "positions" => positions::import_positions(conn, content, overwrite, results).await,
        "switches" => switches::import_switches(conn, content, overwrite, results).await,
        _ => {
            results.push(format!("跳过未知文件: {filename}.csv (支持的文件: network_regions, networks, rooms, workstations, cabinets, positions, switches)"));
            Ok(())
        }
    }
}
