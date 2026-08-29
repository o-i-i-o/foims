//! 设备网卡管理：网卡-网口-IP 层级与整体同步。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::Response;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
use validator::Validate;

use crate::ip::detect_ip_version;
use ipma_auth::meta::{RequestMeta, log_op_best_effort};
use ipma_common::AppJson;
use ipma_common::DbProvider;
use ipma_common::{AppError, msg};
use ipma_models::{
    DeviceInterface, DeviceNetworkConfigSync, IpManager, NetworkCard, NetworkCardSyncItem,
    PortSyncItem,
};

/// 默认网卡名称
pub const DEFAULT_CARD_NAME: &str = "网卡1";
/// 默认网口名称
pub const DEFAULT_PORT_NAME: &str = "eth0";
/// 自动生成网卡（板卡）的排序基值：排在设备模态框手工网卡之后
const AUTO_NIC_SORT_ORDER: i32 = 1000;
/// device_nics.name 列宽（VARCHAR(50)，与校验规则一致）
const NIC_NAME_MAX_CHARS: usize = 50;

/// 按端口名推导自动网卡（板卡）分组前缀。
///
/// 交换机端口名形如 `xg1/0/0/1`（板卡/槽位/端口），取首个 `/` 之前
/// 的板卡段 `xg1`；无 `/` 的名称（如 `eth0`）去掉尾部数字合并同组，
/// 结果为空时回退整名。
pub fn port_group_prefix(port_name: &str) -> &str {
    if let Some(pos) = port_name.find('/') {
        let prefix = &port_name[..pos];
        if !prefix.is_empty() {
            return prefix;
        }
    }
    let trimmed = port_name.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.is_empty() {
        port_name
    } else {
        trimmed
    }
}

/// 取设备的自动分组网卡（不存在则创建），返回网卡 id。
///
/// 网卡名为 `{设备名}-{分组前缀}`（超长按字符截断到列宽），类型 other；
/// 与手工网卡重名时直接复用，保证同名分组落在同一张网卡下。
pub async fn get_or_create_auto_nic(
    tx: &mut PgConnection,
    device_id: Uuid,
    group: &str,
) -> Result<Uuid, AppError> {
    let device_name: String = sqlx::query_scalar("SELECT name FROM devices WHERE id = $1")
        .bind(device_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::NotFound(msg("server.device.not_found")),
            e => AppError::from(e),
        })?;

    let mut nic_name = format!("{device_name}-{group}");
    if nic_name.chars().count() > NIC_NAME_MAX_CHARS {
        nic_name = nic_name.chars().take(NIC_NAME_MAX_CHARS).collect();
    }

    let now = Utc::now();
    let inserted: Option<Uuid> = sqlx::query_scalar(
        r"INSERT INTO device_nics (id, device_id, name, card_type, sort_order, created_at, updated_at)
         VALUES ($1, $2, $3, 'other', $4, $5, $6)
         ON CONFLICT (device_id, name) DO NOTHING
         RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(device_id)
    .bind(&nic_name)
    .bind(AUTO_NIC_SORT_ORDER)
    .bind(now)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(id) = inserted {
        return Ok(id);
    }

    sqlx::query_scalar("SELECT id FROM device_nics WHERE device_id = $1 AND name = $2")
        .bind(device_id)
        .bind(&nic_name)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::from)
}

/// 同步设备的网卡配置（网卡 → 网口 → IP），整体替换
pub async fn sync_device_network_config<P: DbProvider>(
    State(state): State<Arc<P>>,
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
    Ok(ipma_common::ok_json(cards, "server.device.nic.synced"))
}

/// 应用网卡配置：整体替换设备模态框托管的网口（device_managed=TRUE），
/// 保留端口模态框/SNMP 生成的端口（device_managed=FALSE）及其网卡；
/// 清理不再被任何网口引用的网卡。若 cards 为空，则自动生成一张默认
/// 网卡 + 一个默认网口（托管）。提交的网口名与幸存非托管网口重名
/// （或请求内网卡间重名）时返回冲突错误。
///
/// 物理布线（cable_links）仅对本次被移除的托管网口清理：请求中保留
/// port.id 的网口上的布线记录原样保留，避免未改动网口的布线被误删。
pub async fn apply_network_config(
    tx: &mut PgConnection,
    device_id: Uuid,
    room_id: Uuid,
    cards: &[NetworkCardSyncItem],
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    // cards 为空时自动生成默认可管理网卡 + 网口（借用切片，避免整树拷贝）
    let default_cards;
    let effective_cards: &[NetworkCardSyncItem] = if cards.is_empty() {
        default_cards = [default_card_sync_item()];
        &default_cards
    } else {
        cards
    };

    // 幸存的非托管网口（SNMP/端口模态框来源）与本次提交的托管网口共用
    // (device_id, name) 唯一约束：预检重名返回精确错误，避免 INSERT 撞
    // 唯一约束后整个设备保存以通用冲突回滚
    let surviving_names: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT name FROM device_interfaces WHERE device_id = $1 AND NOT device_managed",
    )
    .bind(device_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let mut seen_names: HashSet<&str> = HashSet::new();
    for card in effective_cards {
        for port in &card.ports {
            if !seen_names.insert(port.name.as_str()) || surviving_names.contains(&port.name) {
                return Err(AppError::Conflict(msg(
                    "server.device.interface.name_exists",
                )));
            }
        }
    }

    // 快照托管网口的二层运行属性（端口类型/状态/速率/Trunk id，由端口
    // 模态框或 SNMP 维护）：设备表单整体替换网口后按 id 回填，避免被默认值重置
    let runtime_attrs: HashMap<Uuid, (String, String, Option<String>, Option<i32>)> =
        sqlx::query_as::<_, (Uuid, String, String, Option<String>, Option<i32>)>(
            "SELECT id, port_type, status, speed, trunk_id FROM device_interfaces WHERE device_id = $1 AND device_managed",
        )
        .bind(device_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|(id, port_type, status, speed, trunk_id)| {
            (id, (port_type, status, speed, trunk_id))
        })
        .collect();

    // 删除托管网口的关联数据（顺序：IP → cable_links → 网口）；
    // 非托管网口（SNMP/端口模态框来源）不受设备表单同步影响
    sqlx::query(
        "DELETE FROM ips WHERE device_interface_id IN (
            SELECT id FROM device_interfaces WHERE device_id = $1 AND device_managed)",
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await?;
    // 线缆记录仅对被移除的托管网口清理：先取现存托管网口 id 与请求中
    // 保留的 port.id 求差集，避免幸存网口上的布线被无条件删除
    let managed_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM device_interfaces WHERE device_id = $1 AND device_managed",
    )
    .bind(device_id)
    .fetch_all(&mut *tx)
    .await?;
    let kept_ids: HashSet<Uuid> = effective_cards
        .iter()
        .flat_map(|card| card.ports.iter())
        .filter_map(|port| port.id)
        .collect();
    let removed_ids: Vec<Uuid> = managed_ids
        .iter()
        .filter(|id| !kept_ids.contains(id))
        .copied()
        .collect();
    if !removed_ids.is_empty() {
        sqlx::query(
            r"DELETE FROM cable_links
             WHERE (a_endpoint_type = 'device_interface' AND a_endpoint_id = ANY($1))
                OR (b_endpoint_type = 'device_interface' AND b_endpoint_id = ANY($1))",
        )
        .bind(&removed_ids)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM device_interfaces WHERE device_id = $1 AND device_managed")
        .bind(device_id)
        .execute(&mut *tx)
        .await?;
    // 仅删除不再被任何网口引用的网卡（自动板卡仍被非托管端口引用则保留）
    sqlx::query(
        "DELETE FROM device_nics WHERE device_id = $1 AND id NOT IN (
            SELECT nic_id FROM device_interfaces WHERE device_id = $1 AND nic_id IS NOT NULL)",
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await?;

    for (card_idx, card) in effective_cards.iter().enumerate() {
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
            // VLAN id 合法范围 1..=4094（模型未约束数值范围，handler 兜底）
            if let Some(vlan_id) = port.vlan_id
                && !(1..=4094).contains(&vlan_id)
            {
                return Err(AppError::Validation(
                    msg("server.common.invalid_param").with("param", "vlan_id"),
                ));
            }
            let port_id = port.id.unwrap_or_else(Uuid::new_v4);
            let physical_type = port.physical_type.as_deref().unwrap_or("rj45");
            validate_physical_type(physical_type)?;
            let interface_role = port.interface_role.as_deref().unwrap_or("business");
            validate_interface_role(interface_role)?;
            // 设备表单不含二层属性：存量端口恢复快照值，新端口用默认值
            let (port_type, status, speed, trunk_id) = match runtime_attrs.get(&port_id) {
                Some((pt, st, sp, tid)) => (pt.clone(), st.clone(), sp.clone(), *tid),
                None => ("access".to_string(), "up".to_string(), None, None),
            };

            sqlx::query(
                r"INSERT INTO device_interfaces (id, device_id, nic_id, name, physical_type, interface_role, mac_address, vlan_id, description, sort_order, port_type, status, speed, trunk_id, device_managed, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, TRUE, $15, $16)",
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
            .bind(&port_type)
            .bind(&status)
            .bind(&speed)
            .bind(trunk_id)
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
                    crate::helpers::validate_network_in_room(&mut *tx, room_id, ip.network_id)
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

/// 拉取设备的网卡配置（含嵌套网口、IP）作为 JSON Value。
///
/// 三层各一条批量查询（网卡 / 网口 / IP）后内存组装，
/// 替代原先「每网卡一查、每网口再查 IP」的 N+1 形态。
pub async fn fetch_device_network_config(
    pool: &PgPool,
    device_id: Uuid,
) -> Result<Vec<serde_json::Value>, AppError> {
    let (cards, ports, ip_rows) = tokio::join!(
        sqlx::query_as::<_, NetworkCard>(
            "SELECT * FROM device_nics WHERE device_id = $1 ORDER BY sort_order, name"
        )
        .bind(device_id)
        .fetch_all(pool),
        sqlx::query_as::<_, DeviceInterface>(
            "SELECT * FROM device_interfaces WHERE device_id = $1 ORDER BY sort_order, name",
        )
        .bind(device_id)
        .fetch_all(pool),
        sqlx::query_as::<_, IpManager>(
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
              WHERE di.device_id = $1
              ORDER BY m.ip_address",
        )
        .bind(device_id)
        .fetch_all(pool)
    );

    let cards = cards?;
    let ports = ports?;

    // 网口 ID → 其 IP 列表（批量结果按接口分组）
    let mut ips_by_port: HashMap<Uuid, Vec<IpManager>> = HashMap::new();
    for ip in ip_rows? {
        ips_by_port
            .entry(ip.device_interface_id)
            .or_default()
            .push(ip);
    }

    // 网卡 ID → 网口列表
    let mut ports_by_card: HashMap<Uuid, Vec<DeviceInterface>> = HashMap::new();
    for port in ports {
        if let Some(nic_id) = port.nic_id {
            ports_by_card.entry(nic_id).or_default().push(port);
        }
    }

    let mut result = Vec::new();
    for card in cards {
        let ports_json: Vec<serde_json::Value> = ports_by_card
            .remove(&card.id)
            .unwrap_or_default()
            .into_iter()
            .map(|port| {
                let ips = ips_by_port.remove(&port.id).unwrap_or_default();
                let mut port_json = serde_json::to_value(&port).map_err(|e| {
                    AppError::Internal(msg("server.common.serialize_failed").with("error", e))
                })?;
                port_json["ips"] = serde_json::to_value(&ips).map_err(|e| {
                    AppError::Internal(msg("server.common.serialize_failed").with("error", e))
                })?;
                Ok(port_json)
            })
            .collect::<Result<Vec<_>, AppError>>()?;

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
pub async fn get_device_nics<P: DbProvider>(
    State(state): State<Arc<P>>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let device_type: String = sqlx::query_scalar("SELECT device_type FROM devices WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool()?.get_conn())
        .await?
        .ok_or_else(|| AppError::NotFound(msg("server.device.not_found")))?;

    let cards = fetch_device_network_config(&state.pool()?.get_conn(), id).await?;
    Ok(ipma_common::ok_json(
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

    // ==================== 自动网卡分组 ====================

    #[test]
    fn test_port_group_prefix_slash_names() {
        // 含 / 的端口名取首个板卡段（保留板卡号）
        assert_eq!(port_group_prefix("xg1/0/0/1"), "xg1");
        assert_eq!(
            port_group_prefix("GigabitEthernet1/0/1"),
            "GigabitEthernet1"
        );
        assert_eq!(port_group_prefix("FortyGigE1/0/24"), "FortyGigE1");
    }

    #[test]
    fn test_port_group_prefix_plain_names() {
        // 无 / 的端口名去尾部数字合并同组；纯数字/空串回退整名
        assert_eq!(port_group_prefix("eth0"), "eth");
        assert_eq!(port_group_prefix("eth10"), "eth");
        assert_eq!(port_group_prefix("Vlan-interface"), "Vlan-interface");
        assert_eq!(port_group_prefix("123"), "123");
        assert_eq!(port_group_prefix(""), "");
        // 首段为空的病态输入走去尾数字路径，结果仅影响分组展示
        assert_eq!(port_group_prefix("/0/1"), "/0/");
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
