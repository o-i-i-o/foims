//! 设备物理实体/传感器入库与查询（ENTITY-MIB 采集在 foims-snmp::collect::entity，
//! 本模块保留校验、handler 与 device_entities/device_sensors 落库 SQL）。
//!
//! 台账为当前态快照：同步时覆盖式 upsert + 陈旧 ent_index 清理，不发事件。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use uuid::Uuid;

use foims_common::DbProvider;
use foims_common::log_error;
use foims_common::{AppError, msg};
use foims_models::{DeviceEntity, DeviceSensor};

use foims_snmp::collect::entity::{get_entities_via_snmp, get_sensors_via_snmp};
use foims_snmp::core::get_device_snmp_config;

/// 读库：返回设备已落库的物理实体清单。
pub async fn get_device_entities<P: DbProvider>(
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

    let rows: Vec<DeviceEntity> = sqlx::query_as::<_, DeviceEntity>(
        "SELECT * FROM device_entities WHERE device_id = $1 ORDER BY ent_index",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    Ok(foims_common::ok_json(rows, "server.device.entity.fetched"))
}

/// 读库：返回设备已落库的传感器读数。
pub async fn get_device_sensors<P: DbProvider>(
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

    let rows: Vec<DeviceSensor> = sqlx::query_as::<_, DeviceSensor>(
        "SELECT * FROM device_sensors WHERE device_id = $1 ORDER BY ent_index",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    Ok(foims_common::ok_json(rows, "server.device.sensors.fetched"))
}

/// 采集并落库设备物理实体 + 传感器（一次同步写两张表）。
pub async fn sync_entities_from_snmp<P: DbProvider>(
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
    let entities = get_entities_via_snmp(&params).await.map_err(|e| {
        AppError::Snmp(msg("server.device.snmp.entity_fetch_failed").with("error", e))
    })?;
    let sensors = get_sensors_via_snmp(&params).await.map_err(|e| {
        AppError::Snmp(msg("server.device.snmp.sensor_fetch_failed").with("error", e))
    })?;

    let now = chrono::Utc::now();
    let mut synced = 0usize;
    let mut failed = 0usize;

    let mut tx = conn.begin().await?;

    // 物理实体逐行 upsert（主键 device_id+ent_index）
    let mut seen_ent: Vec<i32> = Vec::with_capacity(entities.len());
    for r in &entities {
        seen_ent.push(r.ent_index);
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO device_entities (id, device_id, ent_index, ent_class, name, descr, model_name, serial_num, mfg_name, hw_rev, fw_rev, sw_rev, is_fru, parent_index, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$15)
               ON CONFLICT (device_id, ent_index) DO UPDATE SET
                    ent_class=EXCLUDED.ent_class, name=EXCLUDED.name, descr=EXCLUDED.descr,
                    model_name=EXCLUDED.model_name, serial_num=EXCLUDED.serial_num,
                    mfg_name=EXCLUDED.mfg_name, hw_rev=EXCLUDED.hw_rev, fw_rev=EXCLUDED.fw_rev,
                    sw_rev=EXCLUDED.sw_rev, is_fru=EXCLUDED.is_fru, parent_index=EXCLUDED.parent_index,
                    updated_at=EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(r.ent_index)
        .bind(&r.ent_class)
        .bind(&r.name)
        .bind(&r.descr)
        .bind(&r.model_name)
        .bind(&r.serial_num)
        .bind(&r.mfg_name)
        .bind(&r.hw_rev)
        .bind(&r.fw_rev)
        .bind(&r.sw_rev)
        .bind(r.is_fru)
        .bind(r.parent_index)
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
                    "log.device.entity.record_write_failed",
                    ent_index = r.ent_index,
                    error = e
                );
            }
        }
    }
    if !seen_ent.is_empty() {
        sqlx::query(
            "DELETE FROM device_entities WHERE device_id = $1 AND NOT (ent_index = ANY($2))",
        )
        .bind(device_id)
        .bind(&seen_ent)
        .execute(&mut *tx)
        .await?;
    }

    // 传感器逐行 upsert
    let mut seen_sensor_ent: Vec<i32> = Vec::with_capacity(sensors.len());
    for r in &sensors {
        seen_sensor_ent.push(r.ent_index);
        let mut sp = sqlx::Acquire::begin(&mut *tx).await?;
        let result = sqlx::query(
            r"INSERT INTO device_sensors (id, device_id, ent_index, sensor_type, sensor_scale, sensor_precision, sensor_value, sensor_status, name, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)
               ON CONFLICT (device_id, ent_index) DO UPDATE SET
                    sensor_type=EXCLUDED.sensor_type, sensor_scale=EXCLUDED.sensor_scale,
                    sensor_precision=EXCLUDED.sensor_precision, sensor_value=EXCLUDED.sensor_value,
                    sensor_status=EXCLUDED.sensor_status, name=EXCLUDED.name,
                    updated_at=EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(device_id)
        .bind(r.ent_index)
        .bind(r.sensor_type)
        .bind(r.sensor_scale)
        .bind(r.sensor_precision)
        .bind(r.sensor_value)
        .bind(r.sensor_status)
        .bind(&r.name)
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
                    "log.device.entity.sensor_write_failed",
                    ent_index = r.ent_index,
                    error = e
                );
            }
        }
    }
    if !seen_sensor_ent.is_empty() {
        sqlx::query(
            "DELETE FROM device_sensors WHERE device_id = $1 AND NOT (ent_index = ANY($2))",
        )
        .bind(device_id)
        .bind(&seen_sensor_ent)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let saved: Vec<DeviceEntity> = sqlx::query_as::<_, DeviceEntity>(
        "SELECT * FROM device_entities WHERE device_id = $1 ORDER BY ent_index",
    )
    .bind(device_id)
    .fetch_all(&conn)
    .await?;

    let message = if failed > 0 {
        msg("server.device.entity.sync_partial")
            .with("synced", synced)
            .with("failed", failed)
    } else if synced > 0 {
        msg("server.device.entity.sync_changed").with("synced", synced)
    } else {
        msg("server.device.entity.sync_unchanged")
    };

    Ok(foims_common::ok_json(saved, message))
}
