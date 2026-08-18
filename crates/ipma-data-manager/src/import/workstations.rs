//! 工位 CSV 导入（含 IP 绑定）。

use crate::import::empty_to_none;
use crate::types::{DataError, DataResult};
use ipma_common::{log_warn, msg};

pub async fn import_workstations(
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
        let ip_address = record.get(2).unwrap_or("").trim();
        let manager = record.get(3).unwrap_or("").trim();
        let description = record.get(4).unwrap_or("").trim();

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

        if manager.len() > 50 {
            results.push(
                msg("server.import_export.row_manager_too_long")
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
            sqlx::query_scalar("SELECT id FROM workstations WHERE name = $1 AND room_id = $2")
                .bind(name)
                .bind(room_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query(
                    "UPDATE workstations SET manager = $1, description = $2, updated_at = NOW() WHERE id = $3",
                )
                .bind(empty_to_none(manager))
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await;

                match update_result {
                    Ok(_) => {
                        handle_workstation_ip(
                            &mut *conn,
                            WorkstationRow {
                                ws_id: id,
                                room_id,
                                ip_address,
                                name,
                                room_name,
                            },
                            true,
                            results,
                        )
                        .await;
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(
                            msg("server.import_export.row_workstation_update_failed")
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
                    msg("server.import_export.workstation_skipped_exists")
                        .with("name", name)
                        .with("room", room_name)
                        .log_string(),
                );
                skip_count += 1;
            }
        } else {
            let new_id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query(
                "INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
            )
            .bind(new_id)
            .bind(name)
            .bind(room_id)
            .bind(empty_to_none(manager))
            .bind(empty_to_none(description))
            .execute(&mut *conn)
            .await;

            match insert_result {
                Ok(_) => {
                    handle_workstation_ip(
                        &mut *conn,
                        WorkstationRow {
                            ws_id: new_id,
                            room_id,
                            ip_address,
                            name,
                            room_name,
                        },
                        false,
                        results,
                    )
                    .await;
                    success_count += 1;
                }
                Err(e) => {
                    results.push(
                        msg("server.import_export.row_workstation_insert_failed")
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
        msg("server.import_export.workstations_summary")
            .with("success", success_count)
            .with("skipped", skip_count)
            .with("failed", error_count)
            .log_string(),
    );
    Ok(())
}

/// 单行工位导入上下文（收敛 `handle_workstation_ip` 的参数）。
struct WorkstationRow<'a> {
    ws_id: uuid::Uuid,
    room_id: uuid::Uuid,
    ip_address: &'a str,
    name: &'a str,
    room_name: &'a str,
}

/// 为工位写入/更新 IP：已绑定 IP 则更新，否则新写入；所在网段按房间推导。
async fn handle_workstation_ip(
    conn: &mut sqlx::PgConnection,
    row: WorkstationRow<'_>,
    is_update: bool,
    results: &mut Vec<String>,
) {
    let WorkstationRow {
        ws_id,
        room_id,
        ip_address,
        name,
        room_name,
    } = row;

    if ip_address.is_empty() {
        let key = if is_update {
            "server.import_export.workstation_updated"
        } else {
            "server.import_export.workstation_imported"
        };
        results.push(
            msg(key)
                .with("name", name)
                .with("room", room_name)
                .log_string(),
        );
        return;
    }

    let existing_ip: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM ips WHERE workstation_id = $1 AND device_type = 'workstation' LIMIT 1",
    )
    .bind(ws_id)
    .fetch_optional(&mut *conn)
    .await
    .unwrap_or_else(|e| {
        log_warn!("log.import.existing_ip_query_failed", error = e);
        None
    });

    let network_id = find_room_network_id(&mut *conn, room_id, ip_address).await;
    let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };
    let key = if is_update {
        "server.import_export.workstation_updated_with_ip"
    } else {
        "server.import_export.workstation_imported_with_ip"
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
                    .with("room", room_name)
                    .with("ip", ip_address)
                    .log_string(),
            ),
            Err(e) => results.push(
                msg("server.import_export.workstation_ip_update_failed")
                    .with("name", name)
                    .with("room", room_name)
                    .with("error", e)
                    .log_string(),
            ),
        }
    } else {
        match sqlx::query(
            "INSERT INTO ips (id, workstation_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'workstation', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(ws_id)
        .bind(network_id)
        .bind(ip_address)
        .bind(ip_version)
        .execute(&mut *conn)
        .await
        {
            Ok(_) => results.push(
                msg(key)
                    .with("name", name)
                    .with("room", room_name)
                    .with("ip", ip_address)
                    .log_string(),
            ),
            Err(e) => results.push(
                msg("server.import_export.workstation_ip_insert_failed")
                    .with("name", name)
                    .with("room", room_name)
                    .with("error", e)
                    .log_string(),
            ),
        }
    }
}

pub(crate) async fn find_room_network_id(
    conn: &mut sqlx::PgConnection,
    room_id: uuid::Uuid,
    ip_address: &str,
) -> Option<uuid::Uuid> {
    match sqlx::query_scalar(
        r"SELECT nc.id
        FROM room_networks rn
        JOIN network_cidrs nc ON rn.network_id = nc.id
        WHERE rn.room_id = $1
        AND (
            (nc.ipv4_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv4_cidr::inet)
            OR (nc.ipv6_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv6_cidr::inet)
        )
        LIMIT 1",
    )
    .bind(room_id)
    .bind(ip_address)
    .fetch_optional(&mut *conn)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            log_warn!("log.import.room_network_query_failed", error = e);
            None
        }
    }
}
