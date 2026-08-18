//! 交换机 CSV 导入（含 SNMP 配置）。

use crate::import::empty_to_none;
use crate::types::{DataError, DataResult};
use ipma_common::{log_warn, msg};

pub async fn import_switches(
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
        let ip_address = record.get(1).unwrap_or("").trim();
        let model = record.get(2).unwrap_or("").trim();
        let vendor = record.get(3).unwrap_or("").trim();
        let location = record.get(4).unwrap_or("").trim();
        let snmp_version = record.get(5).unwrap_or("").trim();
        let snmp_port_str = record.get(6).unwrap_or("").trim();
        let snmp_community = record.get(7).unwrap_or("").trim();
        let snmp_username = record.get(8).unwrap_or("").trim();
        let description = record.get(9).unwrap_or("").trim();

        if name.is_empty() {
            results.push(
                msg("server.import_export.row_name_empty")
                    .with("line", line_num)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if name.len() > 100 {
            results.push(
                msg("server.import_export.row_name_too_long")
                    .with("line", line_num)
                    .with("name", name)
                    .with("max", 100)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if model.len() > 100 {
            results.push(
                msg("server.import_export.row_model_too_long")
                    .with("line", line_num)
                    .with("name", name)
                    .with("max", 100)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if vendor.len() > 50 {
            results.push(
                msg("server.import_export.row_vendor_too_long")
                    .with("line", line_num)
                    .with("name", name)
                    .with("max", 50)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if location.len() > 100 {
            results.push(
                msg("server.import_export.row_location_too_long")
                    .with("line", line_num)
                    .with("name", name)
                    .with("max", 100)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if snmp_community.len() > 100 {
            results.push(
                msg("server.import_export.row_snmp_community_too_long")
                    .with("line", line_num)
                    .with("name", name)
                    .with("max", 100)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if snmp_username.len() > 50 {
            results.push(
                msg("server.import_export.row_snmp_username_too_long")
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

        let snmp_port: i32 = match snmp_port_str.parse() {
            Ok(v) if v > 0 && v <= 65535 => v,
            Ok(v) => {
                results.push(
                    msg("server.import_export.row_snmp_port_out_of_range")
                        .with("line", line_num)
                        .with("name", name)
                        .with("value", v)
                        .log_string(),
                );
                error_count += 1;
                continue;
            }
            Err(_) => 161,
        };

        let snmp_version = match snmp_version {
            "v1" | "v2c" | "v3" => snmp_version.to_string(),
            "" => "v2c".to_string(),
            _ => {
                results.push(
                    msg("server.import_export.row_snmp_version_invalid")
                        .with("line", line_num)
                        .with("name", name)
                        .with("value", snmp_version)
                        .log_string(),
                );
                error_count += 1;
                continue;
            }
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM switches WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query(
                    r"UPDATE switches SET 
                    model = $1, vendor = $2, location = $3, 
                    snmp_version = $4, snmp_port = $5, 
                    snmp_community = $6, snmp_username = $7, 
                    description = $8, updated_at = NOW() WHERE id = $9",
                )
                .bind(empty_to_none(model))
                .bind(empty_to_none(vendor))
                .bind(empty_to_none(location))
                .bind(&snmp_version)
                .bind(snmp_port)
                .bind(empty_to_none(snmp_community))
                .bind(empty_to_none(snmp_username))
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await;

                match update_result {
                    Ok(_) => {
                        handle_switch_ip(&mut *conn, id, ip_address, true, name, results).await;
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(
                            msg("server.import_export.row_switch_update_failed")
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
                    msg("server.import_export.switch_skipped_exists")
                        .with("name", name)
                        .log_string(),
                );
                skip_count += 1;
            }
        } else {
            let id = uuid::Uuid::new_v4();
            let position_id = uuid::Uuid::new_v4();

            if let Err(e) = sqlx::query(
                "INSERT INTO positions (id, name, device_type, created_at, updated_at) VALUES ($1, $2, 'switch', NOW(), NOW())",
            )
            .bind(position_id)
            .bind(name)
            .execute(&mut *conn)
            .await
            {
                log_warn!("log.import.switch_position_create_failed", error = e);
            }

            let insert_result = sqlx::query(
                r"INSERT INTO switches (
                id, name, model, vendor, location, 
                snmp_version, snmp_community, snmp_username, snmp_port, 
                description, created_at, updated_at, position_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW(), $11)",
            )
            .bind(id)
            .bind(name)
            .bind(empty_to_none(model))
            .bind(empty_to_none(vendor))
            .bind(empty_to_none(location))
            .bind(&snmp_version)
            .bind(empty_to_none(snmp_community))
            .bind(empty_to_none(snmp_username))
            .bind(snmp_port)
            .bind(empty_to_none(description))
            .bind(position_id)
            .execute(&mut *conn)
            .await;

            match insert_result {
                Ok(_) => {
                    handle_new_switch_ip(&mut *conn, position_id, ip_address, name, results).await;
                    success_count += 1;
                }
                Err(e) => {
                    results.push(
                        msg("server.import_export.row_switch_insert_failed")
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
        msg("server.import_export.switches_summary")
            .with("success", success_count)
            .with("skipped", skip_count)
            .with("failed", error_count)
            .log_string(),
    );
    Ok(())
}

async fn handle_switch_ip(
    conn: &mut sqlx::PgConnection,
    switch_id: uuid::Uuid,
    ip_address: &str,
    is_update: bool,
    name: &str,
    results: &mut Vec<String>,
) {
    if ip_address.is_empty() {
        let key = if is_update {
            "server.import_export.switch_updated"
        } else {
            "server.import_export.switch_imported"
        };
        results.push(msg(key).with("name", name).log_string());
        return;
    }

    let existing_ip: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM ips WHERE position_id = (SELECT position_id FROM switches WHERE id = $1) LIMIT 1",
    )
    .bind(switch_id)
    .fetch_optional(&mut *conn)
    .await
    .unwrap_or_else(|e| {
        log_warn!("log.import.existing_ip_query_failed", error = e);
        None
    });

    let network_id = find_switch_network_id(&mut *conn, switch_id, ip_address).await;
    let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };

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
                msg("server.import_export.switch_updated_with_ip")
                    .with("name", name)
                    .with("ip", ip_address)
                    .log_string(),
            ),
            Err(e) => results.push(
                msg("server.import_export.switch_ip_update_failed")
                    .with("name", name)
                    .with("error", e)
                    .log_string(),
            ),
        }
    } else {
        match sqlx::query(
            "INSERT INTO ips (id, device_type, network_id, ip_address, ip_version, position_id, status, created_at, updated_at) VALUES ($1, 'cabinet_position', $2, CAST($3 AS INET), $4, (SELECT position_id FROM switches WHERE id = $5), 'active', NOW(), NOW())",
        )
        .bind(uuid::Uuid::new_v4())
        .bind(network_id)
        .bind(ip_address)
        .bind(ip_version)
        .bind(switch_id)
        .execute(&mut *conn)
        .await
        {
            Ok(_) => results.push(
                msg("server.import_export.switch_updated_with_ip")
                    .with("name", name)
                    .with("ip", ip_address)
                    .log_string(),
            ),
            Err(e) => results.push(
                msg("server.import_export.switch_ip_insert_failed")
                    .with("name", name)
                    .with("error", e)
                    .log_string(),
            ),
        }
    }
}

async fn handle_new_switch_ip(
    conn: &mut sqlx::PgConnection,
    position_id: uuid::Uuid,
    ip_address: &str,
    name: &str,
    results: &mut Vec<String>,
) {
    if ip_address.is_empty() {
        results.push(
            msg("server.import_export.switch_imported")
                .with("name", name)
                .log_string(),
        );
        return;
    }

    let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };
    match sqlx::query(
        "INSERT INTO ips (id, device_type, ip_address, ip_version, position_id, status, created_at, updated_at) VALUES ($1, 'cabinet_position', CAST($2 AS INET), $3, $4, 'active', NOW(), NOW())",
    )
    .bind(uuid::Uuid::new_v4())
    .bind(ip_address)
    .bind(ip_version)
    .bind(position_id)
    .execute(&mut *conn)
    .await
    {
        Ok(_) => results.push(
            msg("server.import_export.switch_imported_with_ip")
                .with("name", name)
                .with("ip", ip_address)
                .log_string(),
        ),
        Err(e) => results.push(
            msg("server.import_export.switch_ip_insert_failed")
                .with("name", name)
                .with("error", e)
                .log_string(),
        ),
    }
}

async fn find_switch_network_id(
    conn: &mut sqlx::PgConnection,
    switch_id: uuid::Uuid,
    ip_address: &str,
) -> Option<uuid::Uuid> {
    match sqlx::query_scalar(
        r"SELECT nc.id
        FROM room_networks rn
        JOIN network_cidrs nc ON rn.network_id = nc.id
        JOIN cabinets c ON c.room_id = rn.room_id
        JOIN positions p ON p.cabinet_id = c.id
        JOIN switches s ON s.position_id = p.id
        WHERE s.id = $1
        AND (
            (nc.ipv4_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv4_cidr::inet)
            OR (nc.ipv6_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv6_cidr::inet)
        )
        LIMIT 1",
    )
    .bind(switch_id)
    .bind(ip_address)
    .fetch_optional(&mut *conn)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            log_warn!("log.import.switch_network_query_failed", error = e);
            None
        }
    }
}
