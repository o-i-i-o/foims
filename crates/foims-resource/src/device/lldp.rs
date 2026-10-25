//! 设备 LLDP 邻居入库（SNMP 采集已迁 foims-snmp::collect::lldp，本模块
//! 保留校验、handler 与 device_lldps 落库 SQL）。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::log_error;
use foims_common::{AppError, msg};
use foims_models::{DeviceLldp, LldpNeighbor};

use foims_snmp::collect::lldp::get_lldp_neighbors_via_snmp;
use foims_snmp::core::DeviceForSnmp;
use foims_snmp::core::truncate_to_column_width;

/// device_lldps.local_port 列宽（VARCHAR(50)，与建表契约一致）
const LOCAL_PORT_MAX_CHARS: usize = 50;

/// 获取设备的 LLDP 邻居（先校验设备/IP/SNMP 配置，再走 SNMP 采集）。
pub async fn get_lldp_neighbors(
    pool: &sqlx::PgPool,
    device_id: &Uuid,
) -> Result<Vec<LldpNeighbor>, AppError> {
    let switch = sqlx::query_as::<_, DeviceForSnmp>(
        r"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM devices WHERE id = $1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(msg("server.device.not_found")))?;

    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(i.ip_address) FROM interface_ips i
           JOIN device_interfaces di ON i.device_interface_id = di.id
           WHERE di.device_id = $1
           ORDER BY i.created_at LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    let ip_address = ip_address
        .filter(|ip| !ip.is_empty())
        .ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;

    if switch.snmp_community.is_none() && switch.snmp_username.is_none() {
        return Err(AppError::Validation(msg(
            "server.device.snmp.not_configured",
        )));
    }

    let params = switch.to_snmp_params_async(&ip_address).await?;
    get_lldp_neighbors_via_snmp(&params)
        .await
        .map_err(|e| AppError::Snmp(msg("server.device.snmp.lldp_fetch_failed").with("error", e)))
}

pub async fn get_device_lldp_neighbors<P: DbProvider>(
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

    let lldps: Vec<DeviceLldp> = sqlx::query_as::<_, DeviceLldp>(
        "SELECT * FROM device_lldps WHERE device_id = $1 ORDER BY local_port",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    Ok(foims_common::ok_json(lldps, "server.device.lldp.fetched"))
}

pub async fn sync_lldp_from_snmp<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    // 邻居采集失败（含设备/IP/SNMP 配置问题）已带语义化错误，直接透传
    let neighbors = get_lldp_neighbors(&conn, &device_id).await?;

    let now = chrono::Utc::now();
    let mut synced_count = 0usize;
    let mut failed_count = 0usize;
    // 截断后的本地端口名：入库与过期清理必须使用同一口径
    let mut local_ports: Vec<String> = Vec::with_capacity(neighbors.len());

    // 在同一事务内 upsert 全部邻居并清理已消失的邻居，避免逐条 autocommit 造成部分写入与数据漂移
    let mut tx = conn.begin().await?;

    for neighbor in &neighbors {
        // local_port 列宽 VARCHAR(50)：入库前按字符截断（超宽截断优于必败写入）
        let local_port = truncate_to_column_width(&neighbor.local_port, LOCAL_PORT_MAX_CHARS);
        local_ports.push(local_port.clone());
        // 逐行 SAVEPOINT：单行 upsert 失败仅回滚该行（ROLLBACK TO + RELEASE），
        // 事务保持可用后继续处理后续行；单行失败不再令 PostgreSQL 事务
        // 进入 aborted 状态导致整单 500
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO device_lldps (id, device_id, local_port, neighbor_chassis_id, neighbor_port_id, neighbor_port_desc, neighbor_sys_name, neighbor_sys_desc, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
               ON CONFLICT (device_id, local_port) DO UPDATE SET
                    neighbor_chassis_id = EXCLUDED.neighbor_chassis_id,
                    neighbor_port_id = EXCLUDED.neighbor_port_id,
                    neighbor_port_desc = EXCLUDED.neighbor_port_desc,
                    neighbor_sys_name = EXCLUDED.neighbor_sys_name,
                    neighbor_sys_desc = EXCLUDED.neighbor_sys_desc,
                    updated_at = EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(&local_port)
        .bind(&neighbor.neighbor_chassis_id)
        .bind(&neighbor.neighbor_port_id)
        .bind(&neighbor.neighbor_port_desc)
        .bind(&neighbor.neighbor_sys_name)
        .bind(&neighbor.neighbor_sys_desc)
        .bind(now)
        .execute(&mut *sp)
        .await;

        match result {
            Ok(_) => {
                sp.commit().await?;
                synced_count += 1;
            }
            Err(e) => {
                // 回滚到 SAVEPOINT，恢复本事务可用状态后计入失败并继续
                sp.rollback().await?;
                failed_count += 1;
                log_error!(
                    "log.device.lldp.record_write_failed",
                    port = neighbor.local_port,
                    error = e
                );
            }
        }
    }

    // 清理本次未发现的旧邻居记录（防止数据漂移）；
    // 空采集结果（seen 集为空）时跳过 DELETE 保留存量记录——空结果
    // 视为采集异常而非"邻居全部消失"，与 mac.rs 的 seen_ips 空分支同口径；
    // 清理失败经 ? 向上传播并回滚事务，避免 removed 计数失真与半提交状态
    let stale_removed: i64 = if local_ports.is_empty() {
        0
    } else {
        sqlx::query_scalar(
            "WITH deleted AS (
                DELETE FROM device_lldps WHERE device_id = $1 AND NOT (local_port = ANY($2)) RETURNING 1
             ) SELECT COUNT(*) FROM deleted",
        )
        .bind(device_id)
        .bind(&local_ports)
        .fetch_one(&mut *tx)
        .await?
    };

    tx.commit().await?;

    let saved_lldps: Vec<DeviceLldp> = sqlx::query_as::<_, DeviceLldp>(
        "SELECT * FROM device_lldps WHERE device_id = $1 ORDER BY local_port",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    // 按同步结果构造消息：有失败（部分成功）/ 有变化 / 无变化
    let message = if failed_count > 0 {
        msg("server.device.lldp.sync_partial")
            .with("synced", synced_count)
            .with("failed", failed_count)
            .with("removed", stale_removed)
    } else if synced_count > 0 {
        msg("server.device.lldp.sync_changed")
            .with("synced", synced_count)
            .with("removed", stale_removed)
    } else {
        msg("server.device.lldp.sync_unchanged").with("removed", stale_removed)
    };

    Ok(foims_common::ok_json(saved_lldps, message))
}
