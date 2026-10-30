//! 设备 VLAN 台账入库与查询（Q-BRIDGE-MIB 采集在 foims-snmp::collect::vlan，
//! 本模块保留校验、handler 与 device_vlans 落库 SQL）。配置型台账，只落表不发事件。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::log_error;
use foims_common::{AppError, msg};
use foims_models::DeviceVlan;

use foims_snmp::collect::vlan::get_vlans_via_snmp;
use foims_snmp::core::{get_device_snmp_config, truncate_to_column_width};

/// device_vlans.name 列宽（VARCHAR(64)，与建表契约一致）
const VLAN_NAME_MAX_CHARS: usize = 64;

/// GET /api/resources/devices/{id}/vlans：读库返回设备已落库的 VLAN 台账，
/// 设备不存在 404。
pub async fn get_device_vlans<P: DbProvider>(
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

    let rows: Vec<DeviceVlan> = sqlx::query_as::<_, DeviceVlan>(
        "SELECT * FROM device_vlans WHERE device_id = $1 ORDER BY vlan_id",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    Ok(foims_common::ok_json(rows, "server.device.vlan.fetched"))
}

/// POST /api/resources/devices/{id}/vlans/sync：采集并落库设备 VLAN 台账；
/// 设备未配 IP 或 SNMP 凭据、采集失败均 400。
pub async fn sync_vlans_from_snmp<P: DbProvider>(
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
    let records = get_vlans_via_snmp(&params).await.map_err(|e| {
        AppError::Snmp(msg("server.device.snmp.vlan_fetch_failed").with("error", e))
    })?;

    let now = chrono::Utc::now();
    let mut synced = 0usize;
    let mut failed = 0usize;
    let mut seen: Vec<i32> = Vec::with_capacity(records.len());

    let mut tx = conn.begin().await?;
    for r in &records {
        seen.push(r.vlan_id);
        // name 列宽 VARCHAR(64)：入库前按字符截断（B7，超宽截断优于必败写入）
        let name = r
            .name
            .as_deref()
            .map(|n| truncate_to_column_width(n, VLAN_NAME_MAX_CHARS));
        let member_ports = super::u32_array_json(&r.member_ports);
        let untagged_ports = super::u32_array_json(&r.untagged_ports);
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO device_vlans (id, device_id, vlan_id, name, member_ports, untagged_ports, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$7)
               ON CONFLICT (device_id, vlan_id) DO UPDATE SET
                    name=EXCLUDED.name, member_ports=EXCLUDED.member_ports,
                    untagged_ports=EXCLUDED.untagged_ports, updated_at=EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(r.vlan_id)
        .bind(name)
        .bind(&member_ports)
        .bind(&untagged_ports)
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
                    "log.device.vlan.record_write_failed",
                    vlan_id = r.vlan_id,
                    error = e
                );
            }
        }
    }
    if !seen.is_empty() {
        sqlx::query("DELETE FROM device_vlans WHERE device_id = $1 AND NOT (vlan_id = ANY($2))")
            .bind(device_id)
            .bind(&seen)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    let saved: Vec<DeviceVlan> = sqlx::query_as::<_, DeviceVlan>(
        "SELECT * FROM device_vlans WHERE device_id = $1 ORDER BY vlan_id",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    let message = if failed > 0 {
        msg("server.device.vlan.sync_partial")
            .with("synced", synced)
            .with("failed", failed)
    } else if synced > 0 {
        msg("server.device.vlan.sync_changed").with("synced", synced)
    } else {
        msg("server.device.vlan.sync_unchanged")
    };

    Ok(foims_common::ok_json(saved, message))
}
