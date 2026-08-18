//! 房间 CSV 导入（含网络绑定）。

use crate::import::find_network_id;
use crate::types::{DataError, DataResult};
use ipma_common::{log_warn, msg};

pub async fn import_rooms(
    conn: &mut sqlx::PgConnection,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> DataResult<()> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut error_count = 0;
    let mut line_num = 1;

    for result in rdr.records() {
        line_num += 1;
        let record = result.map_err(|e| {
            DataError::Validation(
                msg("server.import_export.row_parse_failed")
                    .with("line", line_num)
                    .with("error", e),
            )
        })?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let room_type_str = record.get(1).unwrap_or("").trim();

        if name.is_empty() {
            results.push(
                msg("server.import_export.row_name_empty")
                    .with("line", line_num)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if name.len() > 50 {
            results.push(
                msg("server.import_export.row_name_too_long")
                    .with("line", line_num)
                    .with("name", name)
                    .with("max", 50)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        let room_type = match room_type_str {
            "办公室" | "OFFICE" | "office" | "" => "OFFICE",
            "数据中心" | "DATA_CENTER" | "data_center" => "DATA_CENTER",
            "弱电井" | "TELECOM_CLOSET" | "telecom_closet" => "TELECOM_CLOSET",
            _ => {
                results.push(
                    msg("server.import_export.row_room_type_invalid")
                        .with("line", line_num)
                        .with("name", name)
                        .with("type", room_type_str)
                        .log_string(),
                );
                error_count += 1;
                continue;
            }
        };

        let network_names: Vec<&str> = record
            .iter()
            .skip(2)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM rooms WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query(
                    "UPDATE rooms SET room_type = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(room_type)
                .bind(id)
                .execute(&mut *conn)
                .await;

                match update_result {
                    Ok(_) => {
                        let delete_result =
                            sqlx::query("DELETE FROM room_networks WHERE room_id = $1")
                                .bind(id)
                                .execute(&mut *conn)
                                .await;
                        if let Err(e) = delete_result {
                            log_warn!("log.import.room_network_delete_failed", error = e);
                        }

                        let mut linked_networks = Vec::new();
                        for network_name in &network_names {
                            match find_network_id(&mut *conn, network_name).await {
                                Ok(Some(network_id)) => {
                                    let insert_result = sqlx::query(
                                        "INSERT INTO room_networks (id, room_id, network_id, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())",
                                    )
                                    .bind(uuid::Uuid::new_v4())
                                    .bind(id)
                                    .bind(network_id)
                                    .execute(&mut *conn)
                                    .await;
                                    if let Err(e) = insert_result {
                                        log_warn!(
                                            "log.import.room_network_insert_failed",
                                            error = e
                                        );
                                    }
                                    linked_networks.push(*network_name);
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    log_warn!("log.import.network_lookup_failed", error = e);
                                }
                            }
                        }
                        results.push(
                            msg("server.import_export.room_updated")
                                .with("name", name)
                                .with("type", room_type_str)
                                .with("networks", linked_networks.join("; "))
                                .log_string(),
                        );
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(
                            msg("server.import_export.row_room_update_failed")
                                .with("line", line_num)
                                .with("name", name)
                                .with("error", e)
                                .log_string(),
                        );
                        error_count += 1;
                    }
                }
            } else {
                results.push(
                    msg("server.import_export.room_skipped_exists")
                        .with("name", name)
                        .log_string(),
                );
                skip_count += 1;
            }
        } else {
            let id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query(
                "INSERT INTO rooms (id, name, room_type, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())",
            )
            .bind(id)
            .bind(name)
            .bind(room_type)
            .execute(&mut *conn)
            .await;

            match insert_result {
                Ok(_) => {
                    let mut linked_networks = Vec::new();
                    let mut missing_networks = Vec::new();
                    for network_name in &network_names {
                        match find_network_id(&mut *conn, network_name).await {
                            Ok(Some(network_id)) => {
                                if let Err(e) = sqlx::query(
                                    "INSERT INTO room_networks (id, room_id, network_id, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())",
                                )
                                .bind(uuid::Uuid::new_v4())
                                .bind(id)
                                .bind(network_id)
                                .execute(&mut *conn)
                                .await
                                {
                                    log_warn!("log.import.room_network_insert_failed", error = e);
                                }
                                linked_networks.push(*network_name);
                            }
                            Ok(None) => {
                                missing_networks.push(*network_name);
                            }
                            Err(e) => {
                                log_warn!("log.import.network_lookup_failed", error = e);
                                missing_networks.push(*network_name);
                            }
                        }
                    }

                    if !missing_networks.is_empty() {
                        results.push(
                            msg("server.import_export.room_imported_missing_networks")
                                .with("name", name)
                                .with("type", room_type_str)
                                .with("networks", linked_networks.join("; "))
                                .with("missing", missing_networks.join("; "))
                                .log_string(),
                        );
                    } else if linked_networks.is_empty() {
                        results.push(
                            msg("server.import_export.room_imported_no_network")
                                .with("name", name)
                                .with("type", room_type_str)
                                .log_string(),
                        );
                    } else {
                        results.push(
                            msg("server.import_export.room_imported")
                                .with("name", name)
                                .with("type", room_type_str)
                                .with("networks", linked_networks.join("; "))
                                .log_string(),
                        );
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(
                        msg("server.import_export.row_room_insert_failed")
                            .with("line", line_num)
                            .with("name", name)
                            .with("error", e)
                            .log_string(),
                    );
                    error_count += 1;
                }
            }
        }
    }

    results.push(
        msg("server.import_export.rooms_summary")
            .with("success", success_count)
            .with("skipped", skip_count)
            .with("failed", error_count)
            .log_string(),
    );
    Ok(())
}
