//! 网络区域 CSV 导入。

use crate::import::empty_to_none;
use crate::types::{DataError, DataResult};
use ipma_common::msg;

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
        let description = record.get(1).unwrap_or("").trim();

        if name.is_empty() {
            results.push(
                msg("server.import_export.row_name_empty")
                    .with("line", line_num)
                    .log_string(),
            );
            error_count += 1;
            continue;
        }

        if name.len() > 20 {
            results.push(
                msg("server.import_export.row_name_too_long")
                    .with("line", line_num)
                    .with("name", name)
                    .with("max", 20)
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
                .map_err(|e| {
                    DataError::Validation(
                        msg("server.import_export.region_update_failed")
                            .with("name", name)
                            .with("error", e),
                    )
                })?;
                results.push(
                    msg("server.import_export.region_updated")
                        .with("name", name)
                        .log_string(),
                );
                success_count += 1;
            } else {
                results.push(
                    msg("server.import_export.region_skipped_exists")
                        .with("name", name)
                        .log_string(),
                );
                skip_count += 1;
            }
        } else {
            sqlx::query("INSERT INTO network_regions (id, name, description, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                .bind(uuid::Uuid::new_v4())
                .bind(name)
                .bind(empty_to_none(description))
                .execute(&mut *conn)
                .await
                .map_err(|e| {
                    DataError::Validation(
                        msg("server.import_export.region_insert_failed")
                            .with("name", name)
                            .with("error", e),
                    )
                })?;
            results.push(
                msg("server.import_export.region_imported")
                    .with("name", name)
                    .log_string(),
            );
            success_count += 1;
        }
    }

    results.push(
        msg("server.import_export.regions_summary")
            .with("success", success_count)
            .with("skipped", skip_count)
            .with("failed", error_count)
            .log_string(),
    );
    Ok(())
}
