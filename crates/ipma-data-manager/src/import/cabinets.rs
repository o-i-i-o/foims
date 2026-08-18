//! 机柜 CSV 导入。

use crate::import::empty_to_none;
use crate::types::{DataError, DataResult};
use ipma_common::msg;

pub async fn import_cabinets(
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
        let room_name = record.get(1).unwrap_or("").trim();
        let description = record
            .get(record.len().saturating_sub(1))
            .unwrap_or("")
            .trim();

        let network_names: Vec<&str> = record
            .iter()
            .skip(2)
            .take(record.len().saturating_sub(3))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

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

        if description.len() > 255 {
            results.push(
                msg("server.import_export.row_description_too_long")
                    .with("line", line_num)
                    .with("name", name)
                    .with("max", 255)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if room_name.is_empty() {
            results.push(
                msg("server.import_export.row_room_missing")
                    .with("line", line_num)
                    .with("name", name)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        let room_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM rooms WHERE name = $1")
                .bind(room_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        let Some(room_id) = room_id else {
            results.push(
                msg("server.import_export.row_room_not_found")
                    .with("line", line_num)
                    .with("name", name)
                    .with("room", room_name)
                    .log_string(),
            );
            error_count += 1;
            continue;
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM cabinets WHERE name = $1 AND room_id = $2")
                .bind(name)
                .bind(room_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query(
                    "UPDATE cabinets SET description = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await;

                match update_result {
                    Ok(_) => {
                        results.push(
                            msg("server.import_export.cabinet_updated")
                                .with("name", name)
                                .with("room", room_name)
                                .log_string(),
                        );
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(
                            msg("server.import_export.row_cabinet_update_failed")
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
                    msg("server.import_export.cabinet_skipped_exists")
                        .with("name", name)
                        .with("room", room_name)
                        .log_string(),
                );
                skip_count += 1;
            }
        } else {
            let id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query(
                "INSERT INTO cabinets (id, name, room_id, capacity, description, created_at, updated_at) VALUES ($1, $2, $3, 42, $4, NOW(), NOW())",
            )
            .bind(id)
            .bind(name)
            .bind(room_id)
            .bind(empty_to_none(description))
            .execute(&mut *conn)
            .await;

            match insert_result {
                Ok(_) => {
                    if network_names.is_empty() {
                        results.push(
                            msg("server.import_export.cabinet_imported_no_network")
                                .with("name", name)
                                .with("room", room_name)
                                .log_string(),
                        );
                    } else {
                        results.push(
                            msg("server.import_export.cabinet_imported")
                                .with("name", name)
                                .with("room", room_name)
                                .with("network", network_names[0])
                                .log_string(),
                        );
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(
                        msg("server.import_export.row_cabinet_insert_failed")
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
        msg("server.import_export.cabinets_summary")
            .with("success", success_count)
            .with("skipped", skip_count)
            .with("failed", error_count)
            .log_string(),
    );
    Ok(())
}
