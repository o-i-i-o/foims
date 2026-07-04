use crate::import::empty_to_none;
use crate::import::workstations::find_room_network_id;
use crate::types::{DataError, DataResult};
use tracing::warn;

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
        let record =
            result.map_err(|e| DataError::Validation(format!("第{line_num}行解析失败: {e}")))?;

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
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 50 {
            results.push(format!("第{line_num}行跳过: 名称 '{name}' 超过50个字符"));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 机位 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        if cabinet_name.is_empty() {
            results.push(format!("第{line_num}行跳过: 机位 '{name}' - 机柜名称为空"));
            error_count += 1;
            continue;
        }

        if start_u_str.is_empty() {
            results.push(format!("第{line_num}行跳过: 机位 '{name}' - 起始U为空"));
            error_count += 1;
            continue;
        }

        let start_u: i32 = match start_u_str.parse() {
            Ok(v) if (1..=42).contains(&v) => v,
            Ok(v) => {
                results.push(format!(
                    "第{line_num}行跳过: 机位 '{name}' - 起始U '{v}' 超出范围(1-42)"
                ));
                error_count += 1;
                continue;
            }
            Err(_) => {
                results.push(format!(
                    "第{line_num}行跳过: 机位 '{name}' - 起始U '{start_u_str}' 不是有效数字"
                ));
                error_count += 1;
                continue;
            }
        };

        let end_u: i32 = match end_u_str.parse() {
            Ok(v) if v >= start_u && v <= 42 => v,
            Ok(v) => {
                results.push(format!(
                    "第{line_num}行跳过: 机位 '{name}' - 结束U '{v}' 无效(必须 >= 起始U 且 <= 42)"
                ));
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
            results.push(format!(
                "第{line_num}行跳过: 机位 '{name}' - 机柜 '{cabinet_name}' 不存在"
            ));
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
                            id,
                            cabinet_id,
                            room_id,
                            ip_address,
                            true,
                            name,
                            cabinet_name,
                            start_u,
                            end_u,
                            results,
                        )
                        .await;
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!("第{line_num}行跳过: 更新机位 '{name}' 失败 - {e}"));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!("跳过机位（已存在）: {name} (机柜: {cabinet_name})"));
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
                        new_id,
                        cabinet_id,
                        room_id,
                        ip_address,
                        false,
                        name,
                        cabinet_name,
                        start_u,
                        end_u,
                        results,
                    )
                    .await;
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!("第{line_num}行跳过: 插入机位 '{name}' 失败 - {e}"));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "机位导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_position_ip(
    conn: &mut sqlx::PgConnection,
    pos_id: uuid::Uuid,
    cabinet_id: uuid::Uuid,
    room_id: Option<uuid::Uuid>,
    ip_address: &str,
    is_update: bool,
    name: &str,
    cabinet_name: &str,
    start_u: i32,
    end_u: i32,
    results: &mut Vec<String>,
) {
    let action = if is_update { "更新" } else { "导入" };

    if ip_address.is_empty() {
        results.push(format!(
            "{action}机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u})"
        ));
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
        warn!("查询现有IP失败: {}", e);
        None
    });

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
            Ok(_) => results.push(format!("{action}机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP: {ip_address})")),
            Err(e) => results.push(format!("{action}机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP更新失败: {e})")),
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
            Ok(_) => results.push(format!("{action}机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP: {ip_address})")),
            Err(e) => results.push(format!("{action}机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP写入失败: {e})")),
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
            warn!("查询机柜网络失败: {}", e);
            None
        }
    }
}
