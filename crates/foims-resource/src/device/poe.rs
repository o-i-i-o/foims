//! 设备 PoE 供电端口入库与查询（PFN-MIB 采集在 foims-snmp::collect::poe，
//! 本模块保留校验、handler、device_poe_ports 落库 SQL 与供电中断事件产出）。
//!
//! 混合触发：同步时读旧行 → `diff_poe_power` → `record_events`。`poe_port_off`
//! 为 warning，会发站内通知（受 snmp_event 方式开关门控，不发邮件）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::log_error;
use foims_common::{AppError, msg};
use foims_models::DevicePoePort;

use foims_snmp::collect::poe::{diff_poe_power, get_poe_ports_via_snmp};
use foims_snmp::core::{get_device_snmp_config, truncate_to_column_width};
use foims_snmp::events::record_events;

/// device_poe_ports.if_name 列宽（VARCHAR(100)）
const IF_NAME_MAX_CHARS: usize = 100;

/// GET /api/resources/devices/{id}/poe：读库返回设备已落库的 PoE 供电端口，
/// 设备不存在 404。
pub async fn get_device_poe<P: DbProvider>(
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

    let rows: Vec<DevicePoePort> = sqlx::query_as::<_, DevicePoePort>(
        "SELECT * FROM device_poe_ports WHERE device_id = $1 ORDER BY if_index",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    Ok(foims_common::ok_json(rows, "server.device.poe.fetched"))
}

/// POST /api/resources/devices/{id}/poe/sync：采集并落库设备 PoE 供电端口，
/// 同时产出供电中断事件；设备未配 IP 或 SNMP 凭据、采集失败均 400。
pub async fn sync_poe_from_snmp<P: DbProvider>(
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
    let records = get_poe_ports_via_snmp(&params)
        .await
        .map_err(|e| AppError::Snmp(msg("server.device.snmp.poe_fetch_failed").with("error", e)))?;

    // 差值比对基线：upsert 前读旧行，检出供电中断（首轮 prev 空不发事件）
    let prev_rows: Vec<DevicePoePort> =
        sqlx::query_as::<_, DevicePoePort>("SELECT * FROM device_poe_ports WHERE device_id = $1")
            .bind(device_id)
            .fetch_all(&conn)
            .await?;
    let obs_events = diff_poe_power(&prev_rows, &records);

    let now = chrono::Utc::now();
    let mut synced = 0usize;
    let mut failed = 0usize;
    let mut seen: Vec<i32> = Vec::with_capacity(records.len());

    let mut tx = conn.begin().await?;
    for r in &records {
        seen.push(r.if_index);
        let if_name = r
            .if_name
            .as_deref()
            .map(|n| truncate_to_column_width(n, IF_NAME_MAX_CHARS));
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO device_poe_ports (id, device_id, if_index, if_name, admin_status, detection_status, power_class, measured_power_w, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)
               ON CONFLICT (device_id, if_index) DO UPDATE SET
                    if_name=EXCLUDED.if_name, admin_status=EXCLUDED.admin_status,
                    detection_status=EXCLUDED.detection_status, power_class=EXCLUDED.power_class,
                    measured_power_w=EXCLUDED.measured_power_w, updated_at=EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(r.if_index)
        .bind(&if_name)
        .bind(r.admin_status)
        .bind(r.detection_status)
        .bind(r.power_class)
        .bind(r.measured_power_w)
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
                    "log.device.poe.record_write_failed",
                    if_index = r.if_index,
                    error = e
                );
            }
        }
    }
    if !seen.is_empty() {
        sqlx::query(
            "DELETE FROM device_poe_ports WHERE device_id = $1 AND NOT (if_index = ANY($2))",
        )
        .bind(device_id)
        .bind(&seen)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    if let Err(e) = record_events(&conn, device_id, &obs_events).await {
        log_error!(
            "log.snmp.event_record_failed",
            device_id = device_id,
            error = e
        );
    }

    let saved: Vec<DevicePoePort> = sqlx::query_as::<_, DevicePoePort>(
        "SELECT * FROM device_poe_ports WHERE device_id = $1 ORDER BY if_index",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    let message = if failed > 0 {
        msg("server.device.poe.sync_partial")
            .with("synced", synced)
            .with("failed", failed)
    } else if synced > 0 {
        msg("server.device.poe.sync_changed").with("synced", synced)
    } else {
        msg("server.device.poe.sync_unchanged")
    };

    Ok(foims_common::ok_json(saved, message))
}
