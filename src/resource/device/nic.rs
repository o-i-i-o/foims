//! 设备网卡管理：网卡-网口-IP 层级与整体同步。

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
use validator::Validate;

use crate::app_state::AppState;
use crate::error::{AppError, msg};
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
        return Err(AppError::NotFound(msg("server.device.not_found")));
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
    Ok(crate::error::ok_json(cards, "server.device.nic.synced"))
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
    sqlx::query(
        "DELETE FROM ips WHERE device_interface_id IN (SELECT id FROM device_interfaces WHERE device_id = $1)",
    )
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
    sqlx::query("DELETE FROM device_nics WHERE device_id = $1")
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
            r"INSERT INTO device_nics (id, device_id, name, card_type, description, sort_order, created_at, updated_at)
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
                    return Err(AppError::Conflict(
                        msg("server.ip.already_exists").with("ip", &ip.ip_address),
                    ));
                }

                let network_id: Option<Uuid> = if ip.network_id.is_some() {
                    // 显式指定网段时校验其必须属于设备所在房间，确保数据一致性
                    crate::utils::validate_network_in_room(&mut *tx, room_id, ip.network_id)
                        .await?;
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
                    "INSERT INTO ips (id, device_interface_id, network_id, ip_address, ip_version, description, status, last_seen, created_at, updated_at)
                     VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9, $10)",
                )
                .bind(Uuid::new_v4())
                .bind(port_id)
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
        sqlx::query_as("SELECT * FROM device_nics WHERE device_id = $1 ORDER BY sort_order, name")
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
                    m.id, m.device_interface_id, di.device_id, m.network_id,
                    nc.network_region_id AS network_region_id,
                    nc.name AS network_name,
                    nr.name AS network_region,
                    host(m.ip_address) as ip_address,
                    m.ip_version, di.mac_address AS mac_address, m.description,
                    m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ
                  FROM ips m
                  JOIN device_interfaces di ON m.device_interface_id = di.id
                  LEFT JOIN network_cidrs nc ON m.network_id = nc.id
                  LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
                  WHERE m.device_interface_id = $1
                  ORDER BY m.ip_address",
            )
            .bind(port.id)
            .fetch_all(pool)
            .await?;

            let mut port_json = serde_json::to_value(&port).map_err(|e| {
                AppError::Internal(msg("server.common.serialize_failed").with("error", e))
            })?;
            port_json["ips"] = serde_json::to_value(&ips).map_err(|e| {
                AppError::Internal(msg("server.common.serialize_failed").with("error", e))
            })?;
            ports_json.push(port_json);
        }

        let mut card_json = serde_json::to_value(&card).map_err(|e| {
            AppError::Internal(msg("server.common.serialize_failed").with("error", e))
        })?;
        card_json["ports"] = serde_json::to_value(ports_json).map_err(|e| {
            AppError::Internal(msg("server.common.serialize_failed").with("error", e))
        })?;
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
        .ok_or_else(|| AppError::NotFound(msg("server.device.not_found")))?;

    let cards = fetch_device_network_config(&state.pool()?.get_conn(), id).await?;
    Ok(crate::error::ok_json(
        serde_json::json!({ "device_type": device_type, "cards": cards }),
        "server.device.nic.fetched",
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
        return Err(AppError::Validation(msg(
            "server.device.nic.card_type_invalid",
        )));
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
        return Err(AppError::Validation(msg(
            "server.device.nic.physical_type_invalid",
        )));
    }
    Ok(())
}

pub fn validate_interface_role(interface_role: &str) -> Result<(), AppError> {
    if !matches!(
        interface_role,
        "management" | "business" | "loopback" | "uplink" | "other"
    ) {
        return Err(AppError::Validation(msg(
            "server.device.nic.interface_role_invalid",
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== 网卡类型校验 ====================

    #[test]
    fn test_validate_card_type_accepts_all_valid() {
        for card_type in ["pcie", "onboard", "usb", "virtual", "wwan", "wifi", "other"] {
            assert!(
                validate_card_type(card_type).is_ok(),
                "合法网卡类型 {card_type} 应通过校验"
            );
        }
    }

    #[test]
    fn test_validate_card_type_rejects_invalid() {
        // 未收录值、大小写变体、空串均应返回 Validation 错误
        for invalid in ["", "PCIE", "pci-e", "pcie ", "bluetooth", "nvidia"] {
            let result = validate_card_type(invalid);
            let err = result
                .err()
                .unwrap_or_else(|| panic!("非法类型 {invalid:?} 应被拒绝"));
            assert!(
                matches!(err, AppError::Validation(_)),
                "应返回 Validation 错误，实际: {err}"
            );
        }
    }

    // ==================== 物理接口类型校验 ====================

    #[test]
    fn test_validate_physical_type_accepts_all_valid() {
        for physical_type in [
            "rj45",
            "sfp",
            "sfp_plus",
            "sfp28",
            "qsfp_plus",
            "qsfp28",
            "wifi",
            "virtual",
            "other",
        ] {
            assert!(
                validate_physical_type(physical_type).is_ok(),
                "合法物理类型 {physical_type} 应通过校验"
            );
        }
    }

    #[test]
    fn test_validate_physical_type_rejects_invalid() {
        // 常见别名（如 sfp+、QSFP28 大写）不收录，按非法处理
        for invalid in ["", "SFP", "sfp+", "QSFP28", "rj-45", "aix"] {
            let result = validate_physical_type(invalid);
            let err = result
                .err()
                .unwrap_or_else(|| panic!("非法类型 {invalid:?} 应被拒绝"));
            assert!(
                matches!(err, AppError::Validation(_)),
                "应返回 Validation 错误，实际: {err}"
            );
        }
    }

    // ==================== 接口角色校验 ====================

    #[test]
    fn test_validate_interface_role_accepts_all_valid() {
        for role in ["management", "business", "loopback", "uplink", "other"] {
            assert!(
                validate_interface_role(role).is_ok(),
                "合法接口角色 {role} 应通过校验"
            );
        }
    }

    #[test]
    fn test_validate_interface_role_rejects_invalid() {
        for invalid in ["", "Management", "mgmt", "business ", "downlink", "admin"] {
            let result = validate_interface_role(invalid);
            let err = result
                .err()
                .unwrap_or_else(|| panic!("非法角色 {invalid:?} 应被拒绝"));
            assert!(
                matches!(err, AppError::Validation(_)),
                "应返回 Validation 错误，实际: {err}"
            );
        }
    }

    // ==================== 默认网卡配置 ====================

    #[test]
    fn test_default_card_sync_item_matches_constants() {
        // 空同步请求自动生成的默认配置应与常量及默认类型一致
        assert_eq!(DEFAULT_CARD_NAME, "网卡1");
        assert_eq!(DEFAULT_PORT_NAME, "eth0");

        let item = default_card_sync_item();
        assert_eq!(item.name, DEFAULT_CARD_NAME);
        assert_eq!(
            item.card_type.as_deref(),
            Some("pcie"),
            "默认网卡类型应为 pcie"
        );
        assert_eq!(item.id, None, "默认生成项不应携带 id");
        assert_eq!(item.description, None);
        assert_eq!(item.ports.len(), 1, "默认配置应含一个网口");

        let port = item
            .ports
            .first()
            .unwrap_or_else(|| panic!("默认网口应存在"));
        assert_eq!(port.name, DEFAULT_PORT_NAME);
        assert_eq!(
            port.physical_type.as_deref(),
            Some("rj45"),
            "默认物理类型应为 rj45"
        );
        assert_eq!(
            port.interface_role.as_deref(),
            Some("business"),
            "默认角色应为 business"
        );
        assert_eq!(port.id, None);
        assert_eq!(port.mac_address, None);
        assert_eq!(port.vlan_id, None);
        assert!(port.ips.is_empty(), "默认网口不应携带 IP");
    }
}
