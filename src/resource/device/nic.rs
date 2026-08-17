//! 设备网卡管理：网卡-网口-IP 层级与整体同步。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::error::AppError;
use crate::models::{
    DeviceInterface, DeviceNetworkConfigSync, IpManager, NetworkCard, NetworkCardSyncItem,
    PortSyncItem,
};
use crate::resource::ip::detect_ip_version;
use crate::routes::static_files::AppJson;
use crate::utils::common::{RequestMeta, log_op_best_effort};

/// 默认网卡名称
pub const DEFAULT_CARD_NAME: &str = "网卡1";
/// 默认网口名称
pub const DEFAULT_PORT_NAME: &str = "eth0";

/// 同步设备的网卡配置（网卡 → 网口 → IP），整体替换
pub async fn sync_device_network_config(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    meta: RequestMeta,
    AppJson(req): AppJson<DeviceNetworkConfigSync>,
) -> Result<Response, AppError> {
    req.validate()?;

    let mut tx = state.pool()?.get_conn().begin().await?;

    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM devices WHERE id = $1)")
        .bind(device_id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::NotFound("设备不存在".to_string()));
    }

    let room_id: Uuid = sqlx::query_scalar("SELECT room_id FROM devices WHERE id = $1")
        .bind(device_id)
        .fetch_one(&mut *tx)
        .await?;

    let now = Utc::now();
    apply_network_config(&mut tx, device_id, room_id, &req.cards, now).await?;

    tx.commit().await?;

    let details = serde_json::json!({
        "device_id": device_id,
        "card_count": req.cards.len()
    });
    log_op_best_effort(
        &state.pool()?.get_conn(),
        &meta,
        "sync",
        "device_network_config",
        Some(&device_id),
        &details,
    )
    .await;

    let cards = fetch_device_network_config(&state.pool()?.get_conn(), device_id).await?;
    Ok(crate::error::ok_json(cards, "网卡配置同步成功"))
}

/// 应用网卡配置：先删除设备下所有 IP/网口/网卡，再按 cards 重建。
/// 若 cards 为空，则自动生成一张默认网卡 + 一个默认网口（可管理）。
pub async fn apply_network_config(
    tx: &mut PgConnection,
    device_id: Uuid,
    room_id: Uuid,
    cards: &[NetworkCardSyncItem],
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    // 删除现有数据（顺序：IP → cable_links → 网口 → 网卡）
    sqlx::query("DELETE FROM ips WHERE device_id = $1")
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
    // 删除与设备接口相关的电缆链接
    sqlx::query(
        r"DELETE FROM cable_links
         WHERE (a_endpoint_type = 'device_interface' AND a_endpoint_id IN (SELECT id FROM device_interfaces WHERE device_id = $1))
            OR (b_endpoint_type = 'device_interface' AND b_endpoint_id IN (SELECT id FROM device_interfaces WHERE device_id = $1))",
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM device_interfaces WHERE device_id = $1")
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM nics WHERE device_id = $1")
        .bind(device_id)
        .execute(&mut *tx)
        .await?;

    // cards 为空时自动生成默认可管理网卡 + 网口
    let cards_to_insert: Vec<NetworkCardSyncItem> = if cards.is_empty() {
        vec![default_card_sync_item()]
    } else {
        cards.to_vec()
    };

    for (card_idx, card) in cards_to_insert.iter().enumerate() {
        card.validate()?;
        let card_id = card.id.unwrap_or_else(Uuid::new_v4);
        let card_type = card.card_type.as_deref().unwrap_or("pcie");
        validate_card_type(card_type)?;

        sqlx::query(
            r"INSERT INTO nics (id, device_id, name, card_type, description, sort_order, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(card_id)
        .bind(device_id)
        .bind(&card.name)
        .bind(card_type)
        .bind(&card.description)
        .bind(card_idx as i32)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        for (port_idx, port) in card.ports.iter().enumerate() {
            port.validate()?;
            let port_id = port.id.unwrap_or_else(Uuid::new_v4);
            let physical_type = port.physical_type.as_deref().unwrap_or("rj45");
            validate_physical_type(physical_type)?;
            let interface_role = port.interface_role.as_deref().unwrap_or("business");
            validate_interface_role(interface_role)?;

            sqlx::query(
                r"INSERT INTO device_interfaces (id, device_id, nic_id, name, physical_type, interface_role, mac_address, vlan_id, description, sort_order, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
            )
            .bind(port_id)
            .bind(device_id)
            .bind(card_id)
            .bind(&port.name)
            .bind(physical_type)
            .bind(interface_role)
            .bind(&port.mac_address)
            .bind(port.vlan_id)
            .bind(&port.description)
            .bind(port_idx as i32)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;

            for ip in &port.ips {
                ip.validate()?;

                let existing_ip: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM ips WHERE ip_address = CAST($1 AS INET)",
                )
                .bind(&ip.ip_address)
                .fetch_optional(&mut *tx)
                .await?;
                if existing_ip.is_some() {
                    return Err(AppError::Conflict(format!(
                        "IP地址 {} 已存在",
                        ip.ip_address
                    )));
                }

                let network_id: Option<Uuid> = if ip.network_id.is_some() {
                    ip.network_id
                } else {
                    sqlx::query_scalar(
                        r"SELECT nc.id
                            FROM room_networks rn
                            JOIN network_cidrs nc ON rn.network_id = nc.id
                            WHERE rn.room_id = $1
                            AND (
                                (nc.ipv4_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv4_cidr::inet)
                                OR (nc.ipv6_cidr IS NOT NULL AND CAST($2 AS INET) <<= nc.ipv6_cidr::inet)
                            )
                            LIMIT 1",
                    )
                    .bind(room_id)
                    .bind(&ip.ip_address)
                    .fetch_optional(&mut *tx)
                    .await?
                };

                let ip_version = detect_ip_version(&ip.ip_address)?;

                sqlx::query(
                    "INSERT INTO ips (id, device_interface_id, device_id, network_id, ip_address, ip_version, description, status, last_seen, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11)",
                )
                .bind(Uuid::new_v4())
                .bind(port_id)
                .bind(device_id)
                .bind(network_id)
                .bind(&ip.ip_address)
                .bind(ip_version)
                .bind(&ip.description)
                .bind("active")
                .bind(now)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    Ok(())
}

/// 拉取设备的网卡配置（含嵌套网口、IP）作为 JSON Value
pub async fn fetch_device_network_config(
    pool: &PgPool,
    device_id: Uuid,
) -> Result<Vec<serde_json::Value>, AppError> {
    let cards: Vec<NetworkCard> =
        sqlx::query_as("SELECT * FROM nics WHERE device_id = $1 ORDER BY sort_order, name")
            .bind(device_id)
            .fetch_all(pool)
            .await?;

    let mut result = Vec::new();
    for card in cards {
        let ports: Vec<DeviceInterface> = sqlx::query_as(
            "SELECT * FROM device_interfaces WHERE device_id = $1 AND nic_id = $2 ORDER BY sort_order, name",
        )
        .bind(device_id)
        .bind(card.id)
        .fetch_all(pool)
        .await?;

        let mut ports_json = Vec::new();
        for port in ports {
            let ips: Vec<IpManager> = sqlx::query_as(
                r"SELECT
                    m.id, m.device_interface_id, m.device_id, m.network_id,
                    nc.name AS network_name,
                    nr.name AS network_region,
                    host(m.ip_address) as ip_address,
                    m.ip_version, m.mac_address, m.hostname, m.description,
                    m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ, m.last_mac
                  FROM ips m
                  LEFT JOIN network_cidrs nc ON m.network_id = nc.id
                  LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
                  WHERE m.device_interface_id = $1
                  ORDER BY m.ip_address",
            )
            .bind(port.id)
            .fetch_all(pool)
            .await?;

            let mut port_json = serde_json::to_value(&port)
                .map_err(|e| AppError::Internal(format!("序列化网口数据失败: {e}")))?;
            port_json["ips"] = serde_json::to_value(&ips)
                .map_err(|e| AppError::Internal(format!("序列化IP数据失败: {e}")))?;
            ports_json.push(port_json);
        }

        let mut card_json = serde_json::to_value(&card)
            .map_err(|e| AppError::Internal(format!("序列化网卡数据失败: {e}")))?;
        card_json["ports"] = serde_json::to_value(ports_json)
            .map_err(|e| AppError::Internal(format!("序列化网口数据失败: {e}")))?;
        result.push(card_json);
    }

    Ok(result)
}

/// 获取设备的网卡配置（含嵌套网口、IP）—— GET /api/resources/devices/{id}/nics
pub async fn get_device_nics(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let device_type: String = sqlx::query_scalar("SELECT device_type FROM devices WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::NotFound("设备未找到".to_string()))?;

    let cards = fetch_device_network_config(&state.pool()?.get_conn(), id).await?;
    Ok(crate::error::ok_json(
        serde_json::json!({ "device_type": device_type, "cards": cards }),
        "网卡配置获取成功",
    ))
}

fn default_card_sync_item() -> NetworkCardSyncItem {
    NetworkCardSyncItem {
        id: None,
        name: DEFAULT_CARD_NAME.to_string(),
        card_type: Some("pcie".to_string()),
        description: None,
        ports: vec![PortSyncItem {
            id: None,
            name: DEFAULT_PORT_NAME.to_string(),
            physical_type: Some("rj45".to_string()),
            interface_role: Some("business".to_string()),
            mac_address: None,
            vlan_id: None,
            description: None,
            ips: vec![],
        }],
    }
}

fn validate_card_type(card_type: &str) -> Result<(), AppError> {
    if !matches!(
        card_type,
        "pcie" | "onboard" | "usb" | "virtual" | "wwan" | "wifi" | "other"
    ) {
        return Err(AppError::Validation(
            "网卡类型必须是pcie、onboard、usb、virtual、wwan、wifi或other".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_physical_type(physical_type: &str) -> Result<(), AppError> {
    if !matches!(
        physical_type,
        "rj45"
            | "sfp"
            | "sfp_plus"
            | "sfp28"
            | "qsfp_plus"
            | "qsfp28"
            | "wifi"
            | "virtual"
            | "other"
    ) {
        return Err(AppError::Validation(
            "网口物理形态必须是rj45、sfp、sfp_plus、sfp28、qsfp_plus、qsfp28、wifi、virtual或other"
                .to_string(),
        ));
    }
    Ok(())
}

pub fn validate_interface_role(interface_role: &str) -> Result<(), AppError> {
    if !matches!(
        interface_role,
        "management" | "business" | "loopback" | "uplink" | "other"
    ) {
        return Err(AppError::Validation(
            "网口接口角色必须是management、business、loopback、uplink或other".to_string(),
        ));
    }
    Ok(())
}
