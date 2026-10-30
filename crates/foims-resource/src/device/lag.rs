//! 设备 LAG 聚合口台账入库与查询（IEEE8023-LAG-MIB 采集在
//! foims-snmp::collect::lag，本模块保留校验、handler 与 device_lags 落库 SQL）。
//! 配置型台账，只落表不发事件。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::log_error;
use foims_common::{AppError, msg};
use foims_models::DeviceLag;

use foims_snmp::collect::lag::get_lags_via_snmp;
use foims_snmp::core::{get_device_snmp_config, truncate_to_column_width};

/// device_lags.agg_name 列宽（VARCHAR(100)，与建表契约一致）
const AGG_NAME_MAX_CHARS: usize = 100;

/// 将字符串列表构造为 JSONB 数组值。
fn str_array_json(values: &[String]) -> serde_json::Value {
    serde_json::Value::Array(
        values
            .iter()
            .map(|v| serde_json::Value::String(v.clone()))
            .collect(),
    )
}

/// GET /api/resources/devices/{id}/lag：读库返回设备已落库的 LAG 聚合口台账，
/// 设备不存在 404。
pub async fn get_device_lags<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(device_id)
        .fetch_one(&conn)
        .await?;
    if !exists {
        return Err(AppError::NotFound(msg("server.device.not_found")));
    }

    let rows: Vec<DeviceLag> = sqlx::query_as::<_, DeviceLag>(
        "SELECT * FROM device_lags WHERE device_id = $1 ORDER BY agg_ifindex",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    Ok(foims_common::ok_json(rows, "server.device.lag.fetched"))
}

/// POST /api/resources/devices/{id}/lag/sync：采集并落库设备 LAG 聚合口台账；
/// 设备未配 IP 或 SNMP 凭据、采集失败均 400。
pub async fn sync_lags_from_snmp<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let (switch, ip_address) = get_device_snmp_config(&conn, &device_id).await?;
    let ip_address =
        ip_address.ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;
    if switch.snmp_community.is_none() && switch.snmp_username.is_none() {
        return Err(AppError::Validation(msg(
            "server.device.snmp.not_configured",
        )));
    }

    let params = switch.to_snmp_params_async(&ip_address).await?;
    let records = get_lags_via_snmp(&params)
        .await
        .map_err(|e| AppError::Snmp(msg("server.device.snmp.lag_fetch_failed").with("error", e)))?;

    let now = chrono::Utc::now();
    let mut synced = 0usize;
    let mut failed = 0usize;
    let mut seen: Vec<i32> = Vec::with_capacity(records.len());

    let mut tx = conn.begin().await?;
    for r in &records {
        seen.push(r.agg_ifindex);
        // agg_name 列宽 VARCHAR(100)：入库前按字符截断（B7）
        let agg_name = r
            .agg_name
            .as_deref()
            .map(|n| truncate_to_column_width(n, AGG_NAME_MAX_CHARS));
        let member_ifindexes = super::u32_array_json(&r.member_ifindexes);
        let member_names = str_array_json(&r.member_names);
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO device_lags (id, device_id, agg_ifindex, agg_name, member_ifindexes, member_names, lacp_state, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8)
               ON CONFLICT (device_id, agg_ifindex) DO UPDATE SET
                    agg_name=EXCLUDED.agg_name, member_ifindexes=EXCLUDED.member_ifindexes,
                    member_names=EXCLUDED.member_names, lacp_state=EXCLUDED.lacp_state,
                    updated_at=EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(r.agg_ifindex)
        .bind(agg_name)
        .bind(&member_ifindexes)
        .bind(&member_names)
        .bind(&r.lacp_state)
        .bind(now)
        .execute(&mut *sp)
        .await;
        match result {
            Ok(_) => {
                sp.commit().await?;
                synced += 1;
            }
            Err(e) => {
                sp.rollback().await?;
                failed += 1;
                log_error!(
                    "log.device.lag.record_write_failed",
                    agg_ifindex = r.agg_ifindex,
                    error = e
                );
            }
        }
    }
    if !seen.is_empty() {
        sqlx::query("DELETE FROM device_lags WHERE device_id = $1 AND NOT (agg_ifindex = ANY($2))")
            .bind(device_id)
            .bind(&seen)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    let saved: Vec<DeviceLag> = sqlx::query_as::<_, DeviceLag>(
        "SELECT * FROM device_lags WHERE device_id = $1 ORDER BY agg_ifindex",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    let message = if failed > 0 {
        msg("server.device.lag.sync_partial")
            .with("synced", synced)
            .with("failed", failed)
    } else if synced > 0 {
        msg("server.device.lag.sync_changed").with("synced", synced)
    } else {
        msg("server.device.lag.sync_unchanged")
    };

    Ok(foims_common::ok_json(saved, message))
}
