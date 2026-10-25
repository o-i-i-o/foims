//! 设备 MAC 地址表入库与查询（ARP/FDB 纯采集已迁
//! foims-snmp::collect::arp，本模块保留 handler 与 snmp_arp_entries 落库 SQL）。

use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use chrono::Utc;
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::log_error;
use foims_common::{AppError, msg};
use foims_models::SnmpMac;

use foims_snmp::collect::arp::{diff_arp_entries, get_arp_table_via_snmp};
use foims_snmp::core::{get_device_snmp_config, truncate_to_column_width};
use foims_snmp::events::record_events;

/// snmp_arp_entries.interface 列宽（VARCHAR(50)，与建表契约一致）
const INTERFACE_MAX_CHARS: usize = 50;

pub async fn get_device_mac_table<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let conn = state.pool()?.get_conn();

    let (switch, ip_address) = get_device_snmp_config(&conn, &device_id).await?;

    let ip_address =
        ip_address.ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;

    let snmp_params = switch.to_snmp_params_async(&ip_address).await?;

    let entries = get_arp_table_via_snmp(&snmp_params)
        .await
        .map_err(|e| AppError::Snmp(msg("server.device.snmp.arp_fetch_failed").with("error", e)))?;

    // 差值比对：upsert 前读旧行集合 + 登记 IP，产出新增/移除/冲突/未知终端事件
    let old_rows: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT host(ip_address), mac_address FROM snmp_arp_entries WHERE device_id = $1",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;
    let baseline = old_rows.is_empty();
    let registered_ips: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT DISTINCT host(ip_address) FROM interface_ips")
            .fetch_all(&conn)
            .await?
            .into_iter()
            .collect();
    let obs_events = diff_arp_entries(&old_rows, &entries, &registered_ips, baseline);

    let now = Utc::now();
    let mut upserted_count = 0usize;
    let mut failed_count = 0usize;
    let mut seen_ips: Vec<String> = Vec::new();

    let mut tx = conn.begin().await?;

    for entry in &entries {
        let id = Uuid::new_v4();
        seen_ips.push(entry.ip_address.clone());
        // interface 列宽 VARCHAR(50)：入库前按字符截断（超宽截断优于必败写入）
        let interface = entry
            .interface
            .as_deref()
            .map(|iface| truncate_to_column_width(iface, INTERFACE_MAX_CHARS));
        // 逐行 SAVEPOINT：单行 upsert 失败仅回滚该行（ROLLBACK TO + RELEASE），
        // 事务保持可用后继续处理后续行，恢复"sync_partial 部分成功"语义；
        // 此前任一行失败会令 PostgreSQL 事务进入 aborted 状态，后续语句全部失败
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO snmp_arp_entries (id, device_id, ip_address, mac_address, interface, vlan_id, created_at, updated_at)
               VALUES ($1, $2, CAST($3 AS INET), $4, $5, $6, $7, $7)
               ON CONFLICT (device_id, ip_address)
               DO UPDATE SET mac_address = EXCLUDED.mac_address,
                             interface = COALESCE(EXCLUDED.interface, snmp_arp_entries.interface),
                             vlan_id = COALESCE(EXCLUDED.vlan_id, snmp_arp_entries.vlan_id),
                             updated_at = EXCLUDED.updated_at",
        )
        .bind(id)
        .bind(device_id)
        .bind(&entry.ip_address)
        .bind(&entry.mac_address)
        .bind(&interface)
        .bind(entry.vlan_id)
        .bind(now)
        .execute(&mut *sp)
        .await;

        match result {
            Ok(r) => {
                sp.commit().await?;
                if r.rows_affected() > 0 {
                    upserted_count += 1;
                }
            }
            Err(e) => {
                // 回滚到 SAVEPOINT，恢复本事务可用状态后计入失败并继续
                sp.rollback().await?;
                failed_count += 1;
                log_error!(
                    "log.device.mac.record_write_failed",
                    ip = entry.ip_address,
                    mac = entry.mac_address,
                    error = e
                );
            }
        }
    }

    // 删除本次同步中未出现的陈旧记录（A2：采集成功即以太表为准，
    // 空表时 seen_ips 为空数组，NOT IN 恒真→全部清理；此前
    // `!seen_ips.is_empty()` 守卫会让设备清空 ARP 表后旧行永不清理，
    // 陈旧行持续触发 arp_removed 事件不收敛）
    sqlx::query(
        "DELETE FROM snmp_arp_entries WHERE device_id = $1 AND host(ip_address) NOT IN (SELECT unnest($2::text[]))",
    )
    .bind(device_id)
    .bind(&seen_ips)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // 事件落库 + 通知：失败仅记日志，不影响 MAC 同步结果返回（事件表为事实源）
    if let Err(e) = record_events(&conn, device_id, &obs_events).await {
        log_error!(
            "log.snmp.event_record_failed",
            device_id = device_id,
            error = e
        );
    }

    let saved_macs: Vec<SnmpMac> = sqlx::query_as::<_, SnmpMac>(
        r"SELECT id, device_id, host(ip_address) as ip_address, mac_address, interface, vlan_id, created_at, updated_at
           FROM snmp_arp_entries WHERE device_id = $1 ORDER BY ip_address",
    )
    .bind(device_id)
    .fetch_all(&state.pool()?.get_conn())
    .await?;

    // 按同步结果构造消息：有失败 / 全部成功
    let message = if failed_count > 0 {
        msg("server.device.mac.sync_partial")
            .with("synced", upserted_count)
            .with("failed", failed_count)
    } else {
        msg("server.device.mac.synced").with("count", upserted_count)
    };

    Ok(foims_common::ok_json(saved_macs, message))
}

pub async fn get_device_macs_from_db<P: DbProvider>(
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

    let macs: Vec<SnmpMac> = sqlx::query_as::<_, SnmpMac>(
        r"SELECT id, device_id, host(ip_address) as ip_address, mac_address, interface, vlan_id, created_at, updated_at
           FROM snmp_arp_entries WHERE device_id = $1 ORDER BY ip_address",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await
    .map_err(|e| {
        log_error!("log.device.mac.query_failed", error = e);
        AppError::Database(msg("server.error.database"))
    })?;

    Ok(foims_common::ok_json(macs, "server.device.mac.fetched"))
}
