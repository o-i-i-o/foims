use crate::types::{ApiResponse, DataError, DataProvider, DataResult};
use actix_web::{HttpResponse, web};
use futures_util::TryStreamExt;
use serde_json::json;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Arc;
use tracing::warn;

fn empty_to_none(s: &str) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s.trim().to_string())
    }
}

pub async fn import_csv<P: DataProvider>(
    provider: P,
    mut payload: actix_multipart::Multipart,
    query: web::Query<HashMap<String, String>>,
) -> DataResult<HttpResponse> {
    let mode = query
        .get("mode")
        .cloned()
        .unwrap_or_else(|| "skip".to_string());
    let overwrite = mode == "overwrite";

    let mut file_data: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    const MAX_UPLOAD_SIZE: usize = 50 * 1024 * 1024;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|e| DataError::Internal(format!("读取文件失败: {e}")))?
    {
        if field.name() == Some("file") {
            filename = field
                .content_disposition()
                .and_then(|cd| cd.get_filename().map(std::string::ToString::to_string));
            let mut data = Vec::new();
            while let Some(chunk) = field
                .try_next()
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
                    let mut content = String::new();
                    file.read_to_string(&mut content)
                        .map_err(|e| DataError::Internal(format!("读取CSV文件失败: {e}")))?;
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
        if let Err(e) =
            process_csv_by_filename(&mut conn, &table_name, &content, overwrite, &mut results).await
        {
            results.push(format!("导入 {table_name}.csv 失败: {e}"));
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        json!({ "results": results }),
        "导入完成",
    )))
}

async fn process_csv_by_filename(
    conn: &mut sqlx::PgConnection,
    filename: &str,
    content: &str,
    overwrite: bool,
    results: &mut Vec<String>,
) -> DataResult<()> {
    match filename {
        "network_regions" => import_network_regions(conn, content, overwrite, results).await,
        "networks" => import_networks(conn, content, overwrite, results).await,
        "rooms" => import_rooms(conn, content, overwrite, results).await,
        "workstations" => import_workstations(conn, content, overwrite, results).await,
        "cabinets" => import_cabinets(conn, content, overwrite, results).await,
        "positions" => import_positions(conn, content, overwrite, results).await,
        "switches" => import_switches(conn, content, overwrite, results).await,
        _ => {
            results.push(format!("跳过未知文件: {filename}.csv (支持的文件: network_regions, networks, rooms, workstations, cabinets, positions, switches)"));
            Ok(())
        }
    }
}

async fn import_network_regions(
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
        let description = record.get(1).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        if name.len() > 20 {
            results.push(format!("第{line_num}行跳过: 名称 '{name}' 超过20个字符"));
            error_count += 1;
            continue;
        }

        if description.len() > 255 {
            results.push(format!(
                "第{line_num}行跳过: 网络区域 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM network_regions WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                sqlx::query(
                    "UPDATE network_regions SET description = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| DataError::Validation(format!("更新网络区域 '{name}' 失败: {e}")))?;
                results.push(format!("更新网络区域: {name}"));
                success_count += 1;
            } else {
                results.push(format!("跳过网络区域（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            sqlx::query("INSERT INTO network_regions (id, name, description, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                .bind(uuid::Uuid::new_v4())
                .bind(name)
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await
                .map_err(|e| DataError::Validation(format!("插入网络区域 '{name}' 失败: {e}")))?;
            results.push(format!("导入网络区域: {name}"));
            success_count += 1;
        }
    }

    results.push(format!(
        "网络区域导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_networks(
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
        let region_name = record.get(1).unwrap_or("").trim();
        let ipv4_cidr = record.get(2).unwrap_or("").trim();
        let ipv6_cidr = record.get(3).unwrap_or("").trim();
        let ipv4_gateway = record.get(4).unwrap_or("").trim();
        let ipv6_gateway = record.get(5).unwrap_or("").trim();
        let ipv4_dns = record.get(6).unwrap_or("").trim();
        let ipv6_dns = record.get(7).unwrap_or("").trim();
        let description = record.get(8).unwrap_or("").trim();

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
                "第{line_num}行跳过: 网络 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        if region_name.is_empty() {
            results.push(format!("第{line_num}行跳过: 网络 '{name}' - 网络区域为空"));
            error_count += 1;
            continue;
        }

        let region_id: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM network_regions WHERE name = $1")
                .bind(region_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        let Some(region_id) = region_id else {
            results.push(format!(
                "第{line_num}行跳过: 网络 '{name}' - 网络区域 '{region_name}' 不存在"
            ));
            error_count += 1;
            continue;
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM network_cidrs WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                sqlx::query(r"UPDATE network_cidrs SET 
                    network_region_id = $1, 
                    ipv4_cidr = CASE WHEN $2 = '' THEN NULL ELSE CAST($2 AS CIDR) END,
                    ipv6_cidr = CASE WHEN $3 = '' THEN NULL ELSE CAST($3 AS CIDR) END,
                    ipv4_gateway = CASE WHEN $4 = '' THEN NULL ELSE CAST($4 AS INET) END,
                    ipv6_gateway = CASE WHEN $5 = '' THEN NULL ELSE CAST($5 AS INET) END,
                    ipv4_dns = CASE WHEN $6 = '' THEN NULL ELSE CAST(string_to_array($6, ',') AS INET[]) END,
                    ipv6_dns = CASE WHEN $7 = '' THEN NULL ELSE CAST(string_to_array($7, ',') AS INET[]) END,
                    description = $8, updated_at = NOW() WHERE id = $9")
                    .bind(region_id)
                    .bind(empty_to_none(ipv4_cidr))
                    .bind(empty_to_none(ipv6_cidr))
                    .bind(empty_to_none(ipv4_gateway))
                    .bind(empty_to_none(ipv6_gateway))
                    .bind(empty_to_none(ipv4_dns))
                    .bind(empty_to_none(ipv6_dns))
                    .bind(empty_to_none(description))
                    .bind(id)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| DataError::Validation(format!("更新网络 '{name}' 失败: {e}")))?;
                results.push(format!("更新网络: {name}"));
                success_count += 1;
            } else {
                results.push(format!("跳过网络（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            sqlx::query(r"INSERT INTO network_cidrs (id, name, network_region_id, ipv4_cidr, ipv6_cidr, ipv4_gateway, ipv6_gateway, ipv4_dns, ipv6_dns, description, created_at, updated_at) 
                VALUES ($1, $2, $3, 
                CASE WHEN $4 = '' THEN NULL ELSE CAST($4 AS CIDR) END,
                CASE WHEN $5 = '' THEN NULL ELSE CAST($5 AS CIDR) END,
                CASE WHEN $6 = '' THEN NULL ELSE CAST($6 AS INET) END,
                CASE WHEN $7 = '' THEN NULL ELSE CAST($7 AS INET) END,
                CASE WHEN $8 = '' THEN NULL ELSE CAST(string_to_array($8, ',') AS INET[]) END,
                CASE WHEN $9 = '' THEN NULL ELSE CAST(string_to_array($9, ',') AS INET[]) END,
                $10, NOW(), NOW())")
                .bind(uuid::Uuid::new_v4())
                .bind(name)
                .bind(region_id)
                .bind(empty_to_none(ipv4_cidr))
                .bind(empty_to_none(ipv6_cidr))
                .bind(empty_to_none(ipv4_gateway))
                .bind(empty_to_none(ipv6_gateway))
                .bind(empty_to_none(ipv4_dns))
                .bind(empty_to_none(ipv6_dns))
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await
                .map_err(|e| DataError::Validation(format!("插入网络 '{name}' 失败: {e}")))?;
            results.push(format!("导入网络: {name}"));
            success_count += 1;
        }
    }

    results.push(format!(
        "网络导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_rooms(
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

        let room_type = match room_type_str {
            "办公室" | "OFFICE" | "office" | "" => "OFFICE",
            "数据中心" | "DATA_CENTER" | "data_center" => "DATA_CENTER",
            _ => {
                results.push(format!("第{line_num}行跳过: 无效的类型 '{room_type_str}'"));
                error_count += 1;
                continue;
            }
        };

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM rooms WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                sqlx::query("UPDATE rooms SET room_type = $1, updated_at = NOW() WHERE id = $2")
                    .bind(room_type)
                    .bind(id)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| DataError::Validation(format!("更新房间 '{name}' 失败: {e}")))?;
                results.push(format!("更新房间: {name}"));
                success_count += 1;
            } else {
                results.push(format!("跳过房间（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            sqlx::query("INSERT INTO rooms (id, name, room_type, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                .bind(uuid::Uuid::new_v4())
                .bind(name)
                .bind(room_type)
                .execute(&mut *conn)
                .await
                .map_err(|e| DataError::Validation(format!("插入房间 '{name}' 失败: {e}")))?;
            results.push(format!("导入房间: {name}"));
            success_count += 1;
        }
    }

    results.push(format!(
        "房间导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_workstations(
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
                let update_result = sqlx::query("UPDATE workstations SET manager = $1, description = $2, updated_at = NOW() WHERE id = $3")
                    .bind(empty_to_none(manager))
                    .bind(empty_to_none(description))
                    .bind(id)
                    .execute(&mut *conn)
                    .await;

                match update_result {
                    Ok(_) => {
                        if ip_address.is_empty() {
                            results.push(format!("更新工位: {name} (房间: {room_name})"));
                        } else {
                            let existing_ip: Option<uuid::Uuid> = match sqlx::query_scalar(
                                "SELECT id FROM ips WHERE workstation_id = $1 AND device_type = 'workstation' LIMIT 1"
                            )
                            .bind(id)
                            .fetch_optional(&mut *conn)
                            .await {
                                Ok(v) => v,
                                Err(e) => { warn!("查询现有IP失败: {}", e); None }
                            };

                            let network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
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
                            };

                            let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };

                            if let Some(ip_id) = existing_ip {
                                let ip_update_result = sqlx::query(
                                    "UPDATE ips SET ip_address = CAST($1 AS INET), ip_version = $2, network_id = $3, updated_at = NOW() WHERE id = $4"
                                )
                                .bind(ip_address)
                                .bind(ip_version)
                                .bind(network_id)
                                .bind(ip_id)
                                .execute(&mut *conn)
                                .await;

                                match ip_update_result {
                                    Ok(_) => results.push(format!(
                                        "更新工位: {name} (房间: {room_name}, IP: {ip_address})"
                                    )),
                                    Err(e) => results.push(format!(
                                        "更新工位: {name} (房间: {room_name}, IP更新失败: {e})"
                                    )),
                                }
                            } else {
                                let ip_manager_id = uuid::Uuid::new_v4();
                                let ip_insert_result = sqlx::query(
                                    "INSERT INTO ips (id, workstation_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'workstation', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())"
                                )
                                .bind(ip_manager_id)
                                .bind(id)
                                .bind(network_id)
                                .bind(ip_address)
                                .bind(ip_version)
                                .execute(&mut *conn)
                                .await;

                                match ip_insert_result {
                                    Ok(_) => results.push(format!(
                                        "更新工位: {name} (房间: {room_name}, IP: {ip_address})"
                                    )),
                                    Err(e) => results.push(format!(
                                        "更新工位: {name} (房间: {room_name}, IP写入失败: {e})"
                                    )),
                                }
                            }
                        }
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
            let insert_result = sqlx::query("INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, NOW(), NOW())")
                .bind(new_id)
                .bind(name)
                .bind(room_id)
                .bind(empty_to_none(manager))
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await;

            match insert_result {
                Ok(_) => {
                    if ip_address.is_empty() {
                        results.push(format!("导入工位: {name} (房间: {room_name})"));
                    } else {
                        let network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
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
                        };

                        let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };

                        let ip_insert_result = sqlx::query(
                            "INSERT INTO ips (id, workstation_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'workstation', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())"
                        )
                        .bind(uuid::Uuid::new_v4())
                        .bind(new_id)
                        .bind(network_id)
                        .bind(ip_address)
                        .bind(ip_version)
                        .execute(&mut *conn)
                        .await;

                        match ip_insert_result {
                            Ok(_) => results.push(format!(
                                "导入工位: {name} (房间: {room_name}, IP: {ip_address})"
                            )),
                            Err(e) => results.push(format!(
                                "导入工位: {name} (房间: {room_name}, IP写入失败: {e})"
                            )),
                        }
                    }
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

async fn import_cabinets(
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
        let description = record
            .get(record.len().saturating_sub(1))
            .unwrap_or("")
            .trim();

        if name.is_empty() || room_name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称或房间为空"));
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
            results.push(format!("第{line_num}行跳过: 房间 '{room_name}' 不存在"));
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
                sqlx::query(
                    "UPDATE cabinets SET description = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| DataError::Validation(format!("更新机柜 '{name}' 失败: {e}")))?;
                results.push(format!("更新机柜: {name}"));
                success_count += 1;
            } else {
                results.push(format!("跳过机柜（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            sqlx::query("INSERT INTO cabinets (id, name, room_id, capacity, description, created_at, updated_at) VALUES ($1, $2, $3, 42, $4, NOW(), NOW())")
                .bind(uuid::Uuid::new_v4())
                .bind(name)
                .bind(room_id)
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await
                .map_err(|e| DataError::Validation(format!("插入机柜 '{name}' 失败: {e}")))?;
            results.push(format!("导入机柜: {name}"));
            success_count += 1;
        }
    }

    results.push(format!(
        "机柜导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}

async fn import_positions(
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

        let existing: Option<uuid::Uuid> =
            sqlx::query_scalar("SELECT id FROM positions WHERE name = $1 AND cabinet_id = $2")
                .bind(name)
                .bind(cabinet_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(DataError::from)?;

        if let Some(id) = existing {
            if overwrite {
                let update_result = sqlx::query("UPDATE positions SET start_u = $1, end_u = $2, description = $3, updated_at = NOW() WHERE id = $4")
                    .bind(start_u)
                    .bind(end_u)
                    .bind(empty_to_none(description))
                    .bind(id)
                    .execute(&mut *conn)
                    .await;

                match update_result {
                    Ok(_) => {
                        if ip_address.is_empty() {
                            results.push(format!(
                                "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u})"
                            ));
                        } else {
                            let existing_ip: Option<uuid::Uuid> = match sqlx::query_scalar(
                                "SELECT id FROM ips WHERE position_id = $1 AND device_type = 'cabinet_position' LIMIT 1"
                            )
                            .bind(id)
                            .fetch_optional(&mut *conn)
                            .await {
                                Ok(v) => v,
                                Err(e) => { warn!("查询现有IP失败: {}", e); None }
                            };

                            let network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
                                r"SELECT nc.id
                                FROM room_networks rn
                                JOIN network_cidrs nc ON rn.network_id = nc.id
                                JOIN cabinets c ON c.room_id = rn.room_id
                                WHERE c.id = $1
                                AND (
                                    (nc.ipv4_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv4_cidr::inet)
                                    OR (nc.ipv6_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv6_cidr::inet)
                                )
                                LIMIT 1"
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
                            };

                            let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };

                            if let Some(ip_id) = existing_ip {
                                let ip_update_result = sqlx::query(
                                    "UPDATE ips SET ip_address = CAST($1 AS INET), ip_version = $2, network_id = $3, updated_at = NOW() WHERE id = $4"
                                )
                                .bind(ip_address)
                                .bind(ip_version)
                                .bind(network_id)
                                .bind(ip_id)
                                .execute(&mut *conn)
                                .await;

                                match ip_update_result {
                                    Ok(_) => results.push(format!(
                                        "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP: {ip_address})"
                                    )),
                                    Err(e) => results.push(format!(
                                        "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP更新失败: {e})"
                                    )),
                                }
                            } else {
                                let ip_insert_result = sqlx::query(
                                    "INSERT INTO ips (id, position_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'cabinet_position', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())"
                                )
                                .bind(uuid::Uuid::new_v4())
                                .bind(id)
                                .bind(network_id)
                                .bind(ip_address)
                                .bind(ip_version)
                                .execute(&mut *conn)
                                .await;

                                match ip_insert_result {
                                    Ok(_) => results.push(format!(
                                        "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP: {ip_address})"
                                    )),
                                    Err(e) => results.push(format!(
                                        "更新机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP写入失败: {e})"
                                    )),
                                }
                            }
                        }
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!("第{line_num}行跳过: 更新机位 '{name}' 失败 - {e}"));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!("跳过机位（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            let new_id = uuid::Uuid::new_v4();
            let insert_result = sqlx::query("INSERT INTO positions (id, name, cabinet_id, start_u, end_u, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())")
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
                    if ip_address.is_empty() {
                        results.push(format!(
                            "导入机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u})"
                        ));
                    } else {
                        let network_id: Option<uuid::Uuid> = match sqlx::query_scalar(
                            r"SELECT nc.id
                            FROM room_networks rn
                            JOIN network_cidrs nc ON rn.network_id = nc.id
                            JOIN cabinets c ON c.room_id = rn.room_id
                            WHERE c.id = $1
                            AND (
                                (nc.ipv4_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv4_cidr::inet)
                                OR (nc.ipv6_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv6_cidr::inet)
                            )
                            LIMIT 1"
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
                        };

                        let ip_version: i16 = if ip_address.contains(':') { 6 } else { 4 };

                        let ip_insert_result = sqlx::query(
                            "INSERT INTO ips (id, position_id, device_type, network_id, ip_address, ip_version, status, created_at, updated_at) VALUES ($1, $2, 'cabinet_position', $3, CAST($4 AS INET), $5, 'active', NOW(), NOW())"
                        )
                        .bind(uuid::Uuid::new_v4())
                        .bind(new_id)
                        .bind(network_id)
                        .bind(ip_address)
                        .bind(ip_version)
                        .execute(&mut *conn)
                        .await;

                        match ip_insert_result {
                            Ok(_) => results.push(format!(
                                "导入机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP: {ip_address})"
                            )),
                            Err(e) => results.push(format!(
                                "导入机位: {name} (机柜: {cabinet_name}, U{start_u}-U{end_u}, IP写入失败: {e})"
                            )),
                        }
                    }
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

async fn import_switches(
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
        let model = record.get(2).unwrap_or("").trim();
        let vendor = record.get(3).unwrap_or("").trim();
        let location = record.get(4).unwrap_or("").trim();
        let snmp_version = record.get(5).unwrap_or("").trim();
        let snmp_community = record.get(7).unwrap_or("").trim();
        let description = record.get(9).unwrap_or("").trim();

        if name.is_empty() {
            results.push(format!("第{line_num}行跳过: 名称为空"));
            error_count += 1;
            continue;
        }

        let snmp_version = match snmp_version {
            "v1" | "v2c" | "v3" => snmp_version.to_string(),
            "" => "v2c".to_string(),
            _ => {
                results.push(format!(
                    "第{line_num}行跳过: 无效的SNMP版本 '{snmp_version}'"
                ));
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
                sqlx::query(
                    r"UPDATE switches SET 
                    model = $1, vendor = $2, location = $3, 
                    snmp_version = $4, snmp_community = $5, 
                    description = $6, updated_at = NOW() WHERE id = $7",
                )
                .bind(empty_to_none(model))
                .bind(empty_to_none(vendor))
                .bind(empty_to_none(location))
                .bind(&snmp_version)
                .bind(empty_to_none(snmp_community))
                .bind(empty_to_none(description))
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(|e| DataError::Validation(format!("更新交换机 '{name}' 失败: {e}")))?;
                results.push(format!("更新交换机: {name}"));
                success_count += 1;
            } else {
                results.push(format!("跳过交换机（已存在）: {name}"));
                skip_count += 1;
            }
        } else {
            let position_id = uuid::Uuid::new_v4();
            sqlx::query("INSERT INTO positions (id, name, device_type, created_at, updated_at) VALUES ($1, $2, 'switch', NOW(), NOW())")
                .bind(position_id)
                .bind(name)
                .execute(&mut *conn)
                .await
                .ok();

            sqlx::query(r"INSERT INTO switches (id, name, model, vendor, location, snmp_version, snmp_community, snmp_port, description, created_at, updated_at, position_id) VALUES ($1, $2, $3, $4, $5, $6, $7, 161, $8, NOW(), NOW(), $9)")
                .bind(uuid::Uuid::new_v4())
                .bind(name)
                .bind(empty_to_none(model))
                .bind(empty_to_none(vendor))
                .bind(empty_to_none(location))
                .bind(&snmp_version)
                .bind(empty_to_none(snmp_community))
                .bind(empty_to_none(description))
                .bind(position_id)
                .execute(&mut *conn)
                .await
                .map_err(|e| DataError::Validation(format!("插入交换机 '{name}' 失败: {e}")))?;
            results.push(format!("导入交换机: {name}"));
            success_count += 1;
        }
    }

    results.push(format!(
        "交换机导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}
