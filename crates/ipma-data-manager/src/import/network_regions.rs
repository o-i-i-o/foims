use crate::import::empty_to_none;
use crate::types::{DataError, DataResult};

pub async fn import_network_regions(
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
