//! 机柜 CSV 导入。

use crate::import::empty_to_none;
use crate::types::{DataError, DataResult};

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

        let network_names: Vec<&str> = record
            .iter()
            .skip(2)
            .take(record.len().saturating_sub(3))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

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
                "第{line_num}行跳过: 机柜 '{name}' - 描述超过255个字符"
            ));
            error_count += 1;
            continue;
        }

        if room_name.is_empty() {
            results.push(format!("第{line_num}行跳过: 机柜 '{name}' - 房间名称为空"));
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
                "第{line_num}行跳过: 机柜 '{name}' - 房间 '{room_name}' 不存在"
            ));
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
                        results.push(format!("更新机柜: {name} (房间: {room_name})"));
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(format!("第{line_num}行跳过: 更新机柜 '{name}' 失败 - {e}"));
                        error_count += 1;
                    }
                }
            } else {
                results.push(format!("跳过机柜（已存在）: {name} (房间: {room_name})"));
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
                        results.push(format!("导入机柜: {name} (房间: {room_name}, 无网络关联)"));
                    } else {
                        results.push(format!(
                            "导入机柜: {} (房间: {}, 首网络: {})",
                            name, room_name, network_names[0]
                        ));
                    }
                    success_count += 1;
                }
                Err(e) => {
                    results.push(format!("第{line_num}行跳过: 插入机柜 '{name}' 失败 - {e}"));
                    error_count += 1;
                }
            }
        }
    }

    results.push(format!(
        "机柜导入完成: 成功 {success_count}, 跳过 {skip_count}, 失败 {error_count}"
    ));
    Ok(())
}
