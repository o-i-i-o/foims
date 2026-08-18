//! 网段 CSV 导入。

use crate::import::empty_to_none;
use crate::types::{DataError, DataResult};
use ipma_common::msg;

pub async fn import_networks(
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
        let region_name = record.get(1).unwrap_or("").trim();
        let ipv4_cidr = record.get(2).unwrap_or("").trim();
        let ipv6_cidr = record.get(3).unwrap_or("").trim();
        let ipv4_gateway = record.get(4).unwrap_or("").trim();
        let ipv6_gateway = record.get(5).unwrap_or("").trim();
        let ipv4_dns = record.get(6).unwrap_or("").trim();
        let ipv6_dns = record.get(7).unwrap_or("").trim();
        let description = record.get(8).unwrap_or("").trim();

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

        if region_name.is_empty() {
            results.push(
                msg("server.import_export.row_region_missing")
                    .with("line", line_num)
                    .with("name", name)
                    .log_string(),
            );
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
            results.push(
                msg("server.import_export.row_region_not_found")
                    .with("line", line_num)
                    .with("name", name)
                    .with("region", region_name)
                    .log_string(),
            );
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
                let update_result = sqlx::query(r"UPDATE network_cidrs SET 
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
                    .await;

                match update_result {
                    Ok(_) => {
                        results.push(
                            msg("server.import_export.network_updated")
                                .with("name", name)
                                .log_string(),
                        );
                        success_count += 1;
                    }
                    Err(e) => {
                        results.push(
                            msg("server.import_export.row_network_update_failed")
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
                    msg("server.import_export.network_skipped_exists")
                        .with("name", name)
                        .log_string(),
                );
                skip_count += 1;
            }
        } else {
            let insert_result = sqlx::query(r"INSERT INTO network_cidrs (id, name, network_region_id, ipv4_cidr, ipv6_cidr, ipv4_gateway, ipv6_gateway, ipv4_dns, ipv6_dns, description, created_at, updated_at) 
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
                .await;

            match insert_result {
                Ok(_) => {
                    results.push(
                        msg("server.import_export.network_imported")
                            .with("name", name)
                            .log_string(),
                    );
                    success_count += 1;
                }
                Err(e) => {
                    results.push(
                        msg("server.import_export.row_network_insert_failed")
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
        msg("server.import_export.networks_summary")
            .with("success", success_count)
            .with("skipped", skip_count)
            .with("failed", error_count)
            .log_string(),
    );
    Ok(())
}
