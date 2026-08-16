//! 房间 CSV 导入（含网络绑定）。

use crate::import::find_network_id;
use crate::types::{DataError, DataResult};
use tracing::warn;

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
        let record =
            result.map_err(|e| DataError::Validation(format!("第{line_num}行解析失败: {e}")))?;

        if record.get(0).is_some_and(|s| s == "名称") {
            continue;
        }

        let name = record.get(0).unwrap_or("").trim();
        let room_type_str = record.get(1).unwrap_or("").trim();

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

        let room_type = match room_type_str {
            "办公室" | "OFFICE" | "office" | "" => "OFFICE",
            "数据中心" | "DATA_CENTER" | "data_center" => "DATA_CENTER",
            "弱电井" | "TELECOM_CLOSET" | "telecom_closet" => "TELECOM_CLOSET",
            _ => {
                results.push(format!(
                    "第{line_num}行跳过: 房间 '{name}' - 无效的类型 '{room_type_str}'"
                ));
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
                            warn!("操作失败: {}", e);
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
                                        warn!("操作失败: {}", e);
                                    }
                                    linked_networks.push(*network_name);
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    warn!("查找网络失败: {}", e);
                                }
                            }
                        }
                        results.push(format!(
                            "更新房间: {} (类型: {}, 网络: {})",
                            name,
                            room_type_str,
                            linked_networks.join(", ")
                        ));
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!("第{line_num}行跳过: 更新房间 '{name}' 失败 - {e}"));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!("跳过房间（已存在）: {name}"));
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
                                    warn!("关联房间网络失败: {}", e);
                                }
                                linked_networks.push(*network_name);
                            }
                            Ok(None) => {
                                missing_networks.push(*network_name);
                            }
                            Err(e) => {
                                warn!("查找网络失败: {}", e);
                                missing_networks.push(*network_name);
                            }
                        }
                    }

                    if !missing_networks.is_empty() {
                        results.push(format!(
                            "导入房间: {} (类型: {}, 网络: {}, 未找到网络: {})",
                            name,
                            room_type_str,
                            linked_networks.join(", "),
                            missing_networks.join(", ")
                        ));
                    } else if linked_networks.is_empty() {
                        results.push(format!(
                            "导入房间: {name} (类型: {room_type_str}, 无网络关联)"
                        ));
                    } else {
                        results.push(format!(
                            "导入房间: {} (类型: {}, 网络: {})",
                            name,
                            room_type_str,
                            linked_networks.join(", ")
                        ));
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!("第{line_num}行跳过: 插入房间 '{name}' 失败 - {e}"));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "房间导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}
