//! CSV 导入调度：按文件名将上传内容分派到对应实体导入器。

use crate::types::{DataError, DataProvider, DataResult, ok_json};
use axum::extract::{Multipart, Query};
use axum::response::Response;
use ipma_common::{log_error, log_warn, msg};
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

    while let Some(mut field) = payload.next_field().await.map_err(|e| {
        DataError::Internal(msg("server.import_export.file_read_failed").with("error", e))
    })? {
        if field.name() == Some("file") {
            filename = field.file_name().map(std::string::ToString::to_string);
            let mut data = Vec::new();
            while let Some(chunk) = field.chunk().await.map_err(|e| {
                DataError::Internal(msg("server.import_export.chunk_read_failed").with("error", e))
            })? {
                data.extend_from_slice(&chunk);
                if data.len() > MAX_UPLOAD_SIZE {
                    return Err(DataError::Validation(
                        msg("server.import_export.file_too_large")
                            .with("limit", MAX_UPLOAD_SIZE / 1024 / 1024),
                    ));
                }
            }
            file_data = Some(data);
            break;
        }
    }

    let file_data = file_data
        .ok_or_else(|| DataError::Validation(msg("server.import_export.no_file_selected")))?;
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
                let mut file = zip.by_index(i).map_err(|e| {
                    DataError::Internal(
                        msg("server.import_export.zip_entry_read_failed").with("error", e),
                    )
                })?;

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
                    limited.read_to_string(&mut content).map_err(|e| {
                        DataError::Internal(
                            msg("server.import_export.zip_csv_read_failed").with("error", e),
                        )
                    })?;
                    let entry_size = content.len() as u64;
                    total_decompressed = total_decompressed.saturating_add(entry_size);
                    if total_decompressed > MAX_TOTAL_DECOMPRESSED {
                        return Err(DataError::Internal(
                            msg("server.import_export.zip_bomb_detected")
                                .with("limit", MAX_TOTAL_DECOMPRESSED / 1024 / 1024),
                        ));
                    }
                    entries.push((zip_filename.trim_end_matches(".csv").to_string(), content));
                }
            }
        } else {
            let content = String::from_utf8(file_data_clone.as_ref().clone()).map_err(|e| {
                DataError::Validation(
                    msg("server.import_export.utf8_required").with("error", e.utf8_error()),
                )
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
    .map_err(|e| {
        DataError::Internal(msg("server.import_export.zip_decompress_task_failed").with("error", e))
    })??;

    for (table_name, content) in csv_entries {
        let mut tx = conn.begin().await.map_err(DataError::from)?;
        match process_csv_by_filename(&mut tx, &table_name, &content, overwrite, &mut results).await
        {
            Ok(()) => {
                if let Err(e) = tx.commit().await {
                    log_error!(
                        "log.import.tx_commit_failed",
                        file = format!("{table_name}.csv"),
                        error = e
                    );
                    results.push(
                        msg("server.import_export.tx_commit_failed")
                            .with("file", format!("{table_name}.csv"))
                            .log_string(),
                    );
                }
            }
            Err(e) => {
                log_warn!(
                    "log.import.file_failed",
                    file = format!("{table_name}.csv"),
                    error = e.message().log_string()
                );
                results.push(
                    msg("server.import_export.file_import_failed")
                        .with("file", format!("{table_name}.csv"))
                        .with("error", e.message().key())
                        .log_string(),
                );
                if let Err(rb_err) = tx.rollback().await {
                    log_warn!(
                        "log.import.tx_rollback_failed",
                        file = format!("{table_name}.csv"),
                        error = rb_err
                    );
                }
            }
        }
    }

    Ok(ok_json(
        json!({ "results": results }),
        "server.import_export.completed",
    ))
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
            results.push(
                msg("server.import_export.unknown_file_skipped")
                    .with("file", format!("{filename}.csv"))
                    .log_string(),
            );
            Ok(())
        }
    }
}
