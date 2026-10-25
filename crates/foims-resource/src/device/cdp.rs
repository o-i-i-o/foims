//! 设备 CDP 邻居入库与查询（CISCO-CDP-MIB 采集在 foims-snmp::collect::cdp，
//! 本模块保留校验、handler、device_cdps 落库 SQL 与邻居增删事件产出）。
//!
//! 混合触发：同步时读旧行 → `diff_cdp_neighbors` → `record_events`（失败仅
//! 记日志，不影响台账落库）。CDP 增删事件为 info，只落表不通知。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::log_error;
use foims_common::{AppError, msg};
use foims_models::DeviceCdp;

use foims_snmp::collect::cdp::{diff_cdp_neighbors, get_cdp_neighbors_via_snmp};
use foims_snmp::core::{get_device_snmp_config, truncate_to_column_width};
use foims_snmp::events::record_events;

/// device_cdps.local_port 列宽（VARCHAR(50)，与建表契约一致）
const LOCAL_PORT_MAX_CHARS: usize = 50;
/// device_cdps.neighbor_device_id 列宽（VARCHAR(255)）
const NEIGHBOR_ID_MAX_CHARS: usize = 255;

/// 读库：返回设备已落库的 CDP 邻居。
pub async fn get_device_cdps<P: DbProvider>(
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

    let rows: Vec<DeviceCdp> = sqlx::query_as::<_, DeviceCdp>(
        "SELECT * FROM device_cdps WHERE device_id = $1 ORDER BY local_port",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    Ok(foims_common::ok_json(rows, "server.device.cdp.fetched"))
}

/// 采集并落库设备 CDP 邻居，同时产出邻居增删事件。
pub async fn sync_cdps_from_snmp<P: DbProvider>(
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
    let records = get_cdp_neighbors_via_snmp(&params)
        .await
        .map_err(|e| AppError::Snmp(msg("server.device.snmp.cdp_fetch_failed").with("error", e)))?;

    // 差值比对基线：upsert 前读旧行，产出新增/消失事件（与 macs/sync 同构）
    let prev_rows: Vec<DeviceCdp> =
        sqlx::query_as::<_, DeviceCdp>("SELECT * FROM device_cdps WHERE device_id = $1")
            .bind(device_id)
            .fetch_all(&conn)
            .await?;
    let obs_events = diff_cdp_neighbors(&prev_rows, &records);

    let now = chrono::Utc::now();
    let mut synced = 0usize;
    let mut failed = 0usize;
    // 复合键 (local_port, neighbor_device_id) 拼接串：入库与过期清理同一口径
    let mut seen_keys: Vec<String> = Vec::with_capacity(records.len());

    let mut tx = conn.begin().await?;
    for r in &records {
        let local_port = truncate_to_column_width(&r.local_port, LOCAL_PORT_MAX_CHARS);
        let neighbor_device_id = r
            .neighbor_device_id
            .as_deref()
            .map(|d| truncate_to_column_width(d, NEIGHBOR_ID_MAX_CHARS));
        let Some(dev) = neighbor_device_id.clone() else {
            // 无邻居设备 ID 无法作为自然键的一部分，跳过（采集已过滤，此为兜底）
            continue;
        };
        seen_keys.push(format!("{local_port}::{dev}"));

        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO device_cdps (id, device_id, local_port, neighbor_device_id, neighbor_port, neighbor_address, neighbor_platform, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8)
               ON CONFLICT (device_id, local_port, neighbor_device_id) DO UPDATE SET
                    neighbor_port=EXCLUDED.neighbor_port, neighbor_address=EXCLUDED.neighbor_address,
                    neighbor_platform=EXCLUDED.neighbor_platform, updated_at=EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(&local_port)
        .bind(&neighbor_device_id)
        .bind(&r.neighbor_port)
        .bind(&r.neighbor_address)
        .bind(&r.neighbor_platform)
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
                    "log.device.cdp.record_write_failed",
                    port = local_port,
                    error = e
                );
            }
        }
    }
    // 清理本次未发现的旧邻居（空采集结果跳过删除，避免误清存量）
    if !seen_keys.is_empty() {
        sqlx::query(
            r"DELETE FROM device_cdps
               WHERE device_id = $1
                 AND (local_port || '::' || COALESCE(neighbor_device_id, '')) <> ALL($2)",
        )
        .bind(device_id)
        .bind(&seen_keys)
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

    let saved: Vec<DeviceCdp> = sqlx::query_as::<_, DeviceCdp>(
        "SELECT * FROM device_cdps WHERE device_id = $1 ORDER BY local_port",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    let message = if failed > 0 {
        msg("server.device.cdp.sync_partial")
            .with("synced", synced)
            .with("failed", failed)
    } else if synced > 0 {
        msg("server.device.cdp.sync_changed").with("synced", synced)
    } else {
        msg("server.device.cdp.sync_unchanged")
    };

    Ok(foims_common::ok_json(saved, message))
}
