//! 机位 CSV 导入（含 IP 绑定）。

use crate::import::empty_to_none;
use crate::import::workstations::find_room_network_id;
use crate::types::{DataError, DataResult};
use ipma_common::{log_warn, msg};

pub async fn import_positions(
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
        let cabinet_name = record.get(1).unwrap_or("").trim();
        let start_u_str = record.get(2).unwrap_or("").trim();
        let end_u_str = record.get(3).unwrap_or("").trim();
        let ip_address = record.get(4).unwrap_or("").trim();
        let description = record.get(5).unwrap_or("").trim();

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

        if cabinet_name.is_empty() {
            results.push(
                msg("server.import_export.row_cabinet_missing")
                    .with("line", line_num)
                    .with("name", name)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if start_u_str.is_empty() {
            results.push(
                msg("server.import_export.row_start_u_missing")
                    .with("line", line_num)
                    .with("name", name)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        let start_u: i32 = match start_u_str.parse() {
            Ok(v) if (1..=42).contains(&v) => v,
            Ok(v) => {
                results.push(
                    msg("server.import_export.row_start_u_out_of_range")
                        .with("line", line_num)
                        .with("name", name)
                        .with("value", v)
                        .log_string(),
                );
                error_count += 1;
                continue;
            }
            Err(_) => {
                results.push(
                    msg("server.import_export.row_start_u_invalid_number")
                        .with("line", line_num)
                        .with("name", name)
                        .with("value", start_u_str)
                        .log_string(),
                );
                error_count += 1;
                continue;
            }
        };

        let end_u: i32 = match end_u_str.parse() {
            Ok(v) if v >= start_u && v <= 42 => v,
            Ok(v) => {
                results.push(
                    msg("server.import_export.row_end_u_invalid")
                        .with("line", line_num)
                        .with("name", name)
                        .with("value", v)
                        .log_string(),
                );
                error_count += 1;
                continue;
            }
            Err(_) => start_u,
        };

        let cabinet_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM cabinets WHERE name = $1")
                .bind(cabinet_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        let Some(cabinet_id) = cabinet_id else {
            results.push(
                msg("server.import_export.row_cabinet_not_found")
                    .with("line", line_num)
                    .with("name", name)
                    .with("cabinet", cabinet_name)
                    .log_string(),
            );
            error_count += 1;
            continue;
        };

        let room_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT room_id FROM cabinets WHERE id = $1")
                .bind(cabinet_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2")
                .bind(name)
                .bind(cabinet_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query(
                    "UPDATE positions SET start_u = $1, end_u = $2, description = $3, updated_at = NOW() WHERE id = $4",
                )
                .bind(start_u)
                .bind(end_u)
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await;

                match update_result {
                    Ok(_) => {
                        handle_position_ip(
                            &mut *conn,
                            PositionRow {
                                pos_id: id,
                                cabinet_id,
                                room_id,
                                ip_address,
                                name,
                                cabinet_name,
                                start_u,
                                end_u,
                            },
                            true,
                            results,
                        )
                        .await;
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(
                            msg("server.import_export.row_position_update_failed")
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
                    msg("server.import_export.position_skipped_exists")
                        .with("name", name)
                        .with("cabinet", cabinet_name)
                        .log_string(),
                );
                skip_count += 1;
            }
        } else {
            let new_id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query(
                "INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())",
            )
            .bind(new_id)
            .bind(name)
            .bind(cabinet_id)
            .bind(start_u)
            .bind(end_u)
            .bind(empty_to_none(description))
            .execute(&mut *conn)
            .await;

            match insert_result {
                Ok(_) => {
                    handle_position_ip(
                        &mut *conn,
                        PositionRow {
                            pos_id: new_id,
                            cabinet_id,
                            room_id,
                            ip_address,
                            name,
                            cabinet_name,
                            start_u,
                            end_u,
                        },
                        false,
                        results,
                    )
                    .await;
                    success_count += 1;
                }
                Err(e) => {
                    results.push(
                        msg("server.import_export.row_position_insert_failed")
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
        msg("server.import_export.positions_summary")
            .with("success", success_count)
            .with("skipped", skip_count)
            .with("failed", error_count)
            .log_string(),
    );
    Ok(())
}

/// 单行机位导入上下文（收敛 `handle_position_ip` 的参数）。
struct PositionRow<'a> {
    pos_id: uuid::Uuid,
    cabinet_id: uuid::Uuid,
    room_id: Option<uuid::Uuid>,
    ip_address: &'a str,
    name: &'a str,
    cabinet_name: &'a str,
    start_u: i32,
    end_u: i32,
}

/// 为机位写入/更新 IP：已绑定 IP 则更新，否则新写入；所在网段按房间/机柜推导。
async fn handle_position_ip(
    conn: &mut sqlx::PgConnection,
    row: PositionRow<'_>,
    is_update: bool,
    results: &mut Vec<String>,
) {
    let PositionRow {
        pos_id,
        cabinet_id,
        room_id,
        ip_address,
        name,
        cabinet_name,
        start_u,
        end_u,
    } = row;

    if ip_address.is_empty() {
        let key = if is_update {
            "server.import_export.position_updated"
        } else {
            "server.import_export.position_imported"
        };
        results.push(
            msg(key)
                .with("name", name)
                .with("cabinet", cabinet_name)
                .with("start_u", start_u)
                .with("end_u", end_u)
                .log_string(),
        );
        return;
    }

    let network_id = if let Some(rid) = room_id {
        find_room_network_id(&mut *conn, rid, ip_address).await
    } else {
        find_cabinet_network_id(&mut *conn, cabinet_id, ip_address).await
    };

    let existing_ip: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM ips WHERE position_id = $1 AND device_type = 'cabinet_position' LIMIT 1",
    )
    .bind(pos_id)
    .fetch_optional(&mut *conn)
    .await
    .unwrap_or_else(|e| {
        log_warn!("log.import.existing_ip_query_failed", error = e);
        None
    });

    let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };
    let key = if is_update {
        "server.import_export.position_updated_with_ip"
    } else {
        "server.import_export.position_imported_with_ip"
    };

    if let Some(ip_id) = existing_ip {
        match sqlx::query(
            "UPDATE ips SET ip_address = CAST($1 AS INET), ip_version = $2, network_id = $3, updated_at = NOW() WHERE id = $4",
        )
        .bind(ip_address)
        .bind(ip_version)
        .bind(network_id)
        .bind(ip_id)
        .execute(&mut *conn)
        .await
        {
            Ok(_) => results.push(
                msg(key)
                    .with("name", name)
                    .with("cabinet", cabinet_name)
                    .with("start_u", start_u)
                    .with("end_u", end_u)
                    .with("ip", ip_address)
                    .log_string(),
            ),
            Err(e) => results.push(
                msg("server.import_export.position_ip_update_failed")
                    .with("name", name)
                    .with("cabinet", cabinet_name)
                    .with("start_u", start_u)
                    .with("end_u", end_u)
                    .with("error", e)
                    .log_string(),
            ),
        }
    } else {
        match sqlx::query(
            "INSERT INTO ips (id, position_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'cabinet_position', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(pos_id)
        .bind(network_id)
        .bind(ip_address)
        .bind(ip_version)
        .execute(&mut *conn)
        .await
        {
            Ok(_) => results.push(
                msg(key)
                    .with("name", name)
                    .with("cabinet", cabinet_name)
                    .with("start_u", start_u)
                    .with("end_u", end_u)
                    .with("ip", ip_address)
                    .log_string(),
            ),
            Err(e) => results.push(
                msg("server.import_export.position_ip_insert_failed")
                    .with("name", name)
                    .with("cabinet", cabinet_name)
                    .with("start_u", start_u)
                    .with("end_u", end_u)
                    .with("error", e)
                    .log_string(),
            ),
        }
    }
}

async fn find_cabinet_network_id(
    conn: &mut sqlx::PgConnection,
    cabinet_id: uuid::Uuid,
    ip_address: &str,
) -> Option<uuid::Uuid> {
    match sqlx::query_scalar(
        r"SELECT nc.id
        FROM room_networks rn
        JOIN network_cidrs nc ON rn.network_id = nc.id
        JOIN cabinets c ON c.room_id = rn.room_id
        WHERE c.id = $1
        AND (
            (nc.ipv4_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv4_cidr::inet)
            OR (nc.ipv6_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv6_cidr::inet)
        )
        LIMIT 1",
    )
    .bind(cabinet_id)
    .bind(ip_address)
    .fetch_optional(&mut *conn)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            log_warn!("log.import.cabinet_network_query_failed", error = e);
            None
        }
    }
}
