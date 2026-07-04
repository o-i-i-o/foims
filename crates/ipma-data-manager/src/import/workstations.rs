use crate::import::empty_to_none;
use crate::types::{DataError, DataResult};
use tracing::warn;

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
        let record =
            result.map_err(|e| DataError::Validation(format!("第{line_num}行解析失败: {e}")))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let room_name = record.get(1).unwrap_or("").trim();
        let ip_address = record.get(2).unwrap_or("").trim();
        let manager = record.get(3).unwrap_or("").trim();
        let description = record.get(4).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 50 {
            results.push(format!("第{line_num}行跳过: 名称 '{name}' 超过50个字符"));
            error_count += 1;
            continue;
        }

        if manager.len() > 50 {
            results.push(format!(
                "第{line_num}行跳过: 工位 '{name}' - 负责人超过50个字符"
            ));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 工位 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        if room_name.is_empty() {
            results.push(format!("第{line_num}行跳过: 工位 '{name}' - 房间名称为空"));
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
            results.push(format!(
                "第{line_num}行跳过: 工位 '{name}' - 房间 '{room_name}' 不存在"
            ));
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
                            &mut *conn, id, room_id, ip_address, true, name, room_name, results,
                        )
                        .await;
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!("第{line_num}行跳过: 更新工位 '{name}' 失败 - {e}"));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!("跳过工位（已存在）: {name} (房间: {room_name})"));
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
                        &mut *conn, new_id, room_id, ip_address, false, name, room_name, results,
                    )
                    .await;
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!("第{line_num}行跳过: 插入工位 '{name}' 失败 - {e}"));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "工位导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_workstation_ip(
    conn: &mut sqlx::PgConnection,
    ws_id: uuid::Uuid,
    room_id: uuid::Uuid,
    ip_address: &str,
    is_update: bool,
    name: &str,
    room_name: &str,
    results: &mut Vec<String>,
) {
    if ip_address.is_empty() {
        let action = if is_update { "更新" } else { "导入" };
        results.push(format!("{action}工位: {name} (房间: {room_name})"));
        return;
    }

    let existing_ip: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM ips WHERE workstation_id = $1 AND device_type = 'workstation' LIMIT 1",
    )
    .bind(ws_id)
    .fetch_optional(&mut *conn)
    .await
    .unwrap_or_else(|e| {
        warn!("查询现有IP失败: {}", e);
        None
    });

    let network_id = find_room_network_id(&mut *conn, room_id, ip_address).await;
    let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };
    let action = if is_update { "更新" } else { "导入" };

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
            Ok(_) => results.push(format!("{action}工位: {name} (房间: {room_name}, IP: {ip_address})")),
            Err(e) => results.push(format!("{action}工位: {name} (房间: {room_name}, IP更新失败: {e})")),
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
            Ok(_) => results.push(format!("{action}工位: {name} (房间: {room_name}, IP: {ip_address})")),
            Err(e) => results.push(format!("{action}工位: {name} (房间: {room_name}, IP写入失败: {e})")),
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
            warn!("查询房间网络失败: {}", e);
            None
        }
    }
}
