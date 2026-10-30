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
use foims_auth::meta::{RequestMeta, log_op_best_effort};
use foims_common::AppJson;
use foims_common::DbProvider;
use foims_common::{AppError, msg};
use foims_models::{
    DeviceInterface, DeviceNetworkConfigSync, IpDetail, NetworkCard, NetworkCardSyncItem,
};

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

/// PUT /api/resources/devices/{id}/network-config：同步设备的网卡配置
/// （网卡 → 网口 → IP），按 id 差量同步；整批同事务，设备不存在 404、
/// 空 cards 或 `port_id` 重复/跨设备改挂 400、网口重名 409。
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
    Ok(foims_common::ok_json(cards, "server.device.nic.synced"))
}

/// 应用网卡配置：差量同步设备模态框托管的网口（device_managed=TRUE），
/// 保留端口模态框/SNMP 生成的端口（device_managed=FALSE）及其网卡；
/// 清理不再被任何网口引用的网卡。cards 为空属于非法请求，直接拒绝，
/// 由调用方提交真实的网卡配置，不提供硬编码默认网卡/网口。
/// 提交的网口名与幸存非托管网口重名
/// （或请求内网卡间重名）时返回冲突错误。
///
/// 差量口径：请求携带 `id` 的网卡/网口原地更新（id、created_at 与
/// 二层运行属性保持不变，IP 的 status/last_seen 也不因保存而重置），
/// 未出现的托管网口删除；被删网口上的物理布线（cable_links）随之清理。
/// MAC 统一归一为 `AA:BB:CC:DD:EE:FF` 后落库，并做全局唯一校验。
pub async fn apply_network_config(
    tx: &mut PgConnection,
    device_id: Uuid,
    room_id: Uuid,
    cards: &[NetworkCardSyncItem],
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    validate_cards_not_empty(cards)?;

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
    for card in cards {
        for port in &card.ports {
            if !seen_names.insert(port.name.as_str()) || surviving_names.contains(&port.name) {
                return Err(AppError::Conflict(msg(
                    "server.device.interface.name_exists",
                )));
            }
        }
    }

    // 现存托管网口 id 与请求保留的 port.id 求差集，只删除真正被移除的网口：
    // 删除网口会级联清空其 interface_ips，重建时必须把 IP 的 status/
    // last_seen/created_at/id 全部重置（生命周期语义丢失），故保留网口原地
    // 更新，IP 另按地址做差量同步。
    let managed_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM device_interfaces WHERE device_id = $1 AND device_managed",
    )
    .bind(device_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut kept_ids: HashSet<Uuid> = HashSet::new();
    for port in cards.iter().flat_map(|card| card.ports.iter()) {
        // 同一 port.id 重复提交会让后一段的 IP 差量基准为空、重复插入，直接拒绝
        if let Some(id) = port.id
            && !kept_ids.insert(id)
        {
            return Err(AppError::Validation(
                msg("server.common.invalid_param").with("param", "port_id"),
            ));
        }
    }
    // 保留的 port.id 必须属于本设备的托管网口（防跨设备改挂）
    if let Some(foreign) = kept_ids.iter().find(|id| !managed_ids.contains(id)) {
        return Err(AppError::Validation(
            msg("server.device.nic.port_not_owned").with("port_id", foreign),
        ));
    }
    let removed_ids: Vec<Uuid> = managed_ids
        .iter()
        .filter(|id| !kept_ids.contains(id))
        .copied()
        .collect();
    if !removed_ids.is_empty() {
        // cable_links 无外键、由防删触发器守护：先清引用再删网口
        sqlx::query(
            r"DELETE FROM cable_links
             WHERE (a_endpoint_type = 'device_interface' AND a_endpoint_id = ANY($1))
                OR (b_endpoint_type = 'device_interface' AND b_endpoint_id = ANY($1))",
        )
        .bind(&removed_ids)
        .execute(&mut *tx)
        .await?;
        // 网口删除后其 IP 由外键级联清理（被移除网口不再有 IP 宿主）
        sqlx::query("DELETE FROM device_interfaces WHERE id = ANY($1)")
            .bind(&removed_ids)
            .execute(&mut *tx)
            .await?;
    }
    // 仅删除不再被任何网口引用的网卡（自动板卡仍被非托管端口引用则保留）
    sqlx::query(
        "DELETE FROM device_nics WHERE device_id = $1 AND id NOT IN (
            SELECT nic_id FROM device_interfaces WHERE device_id = $1 AND nic_id IS NOT NULL)",
    )
    .bind(device_id)
    .execute(&mut *tx)
    .await?;

    // 保留网口的现有 IP 快照（按网口分组）：差量同步以地址为键，命中者
    // 仅更新子网归属，status/last_seen/created_at/id 保持原值。
    let kept_ids_vec: Vec<Uuid> = kept_ids.iter().copied().collect();
    let existing_ip_rows: Vec<(Uuid, Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT id, device_interface_id, host(ip_address), subnet_id FROM interface_ips
         WHERE device_interface_id = ANY($1)",
    )
    .bind(&kept_ids_vec)
    .fetch_all(&mut *tx)
    .await?;
    let mut existing_ips: HashMap<Uuid, Vec<(Uuid, String, Uuid)>> = HashMap::new();
    for (ip_id, port_id, ip_addr, subnet_id) in existing_ip_rows {
        existing_ips
            .entry(port_id)
            .or_default()
            .push((ip_id, ip_addr, subnet_id));
    }

    // 请求内 MAC 去重（归一后比对）：同一物理 MAC 不得在一次提交中占两个网口，
    // 与库内唯一索引同口径前置拦截
    let mut seen_macs: HashSet<String> = HashSet::new();

    for (card_idx, card) in cards.iter().enumerate() {
        card.validate()?;
        let card_id = card.id.unwrap_or_else(Uuid::new_v4);
        let card_type = card.card_type.as_deref().unwrap_or("pcie");
        validate_card_type(card_type)?;

        // 保留的网卡仍被网口引用，不能删后重建，改为原地 upsert；
        // 提交的 card.id 必须属于本设备（防跨设备改挂）
        if card.id.is_some() {
            let owned: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM device_nics WHERE id = $1 AND device_id = $2)",
            )
            .bind(card_id)
            .bind(device_id)
            .fetch_one(&mut *tx)
            .await?;
            if !owned {
                return Err(AppError::Validation(
                    msg("server.device.nic.card_not_owned").with("card_id", card_id),
                ));
            }
        }

        sqlx::query(
            r"INSERT INTO device_nics (id, device_id, name, card_type, description, sort_order, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE SET
                 name = EXCLUDED.name,
                 card_type = EXCLUDED.card_type,
                 description = EXCLUDED.description,
                 sort_order = EXCLUDED.sort_order,
                 updated_at = EXCLUDED.updated_at",
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
        .await
        .map_err(|e| {
            // id/name 冲突（UNIQUE(device_id, name) / 主键）映射为 409，
            // 避免并发同步或重复提交时以 500 暴露内部错误
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.is_unique_violation()
            {
                return AppError::Conflict(msg("server.db.conflict"));
            }
            AppError::from(e)
        })?;

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
            let mac_address = crate::helpers::normalize_mac_input(port.mac_address.as_deref())?;
            if let Some(mac) = mac_address.as_deref() {
                if !seen_macs.insert(foims_common::net::normalize_mac(mac)) {
                    return Err(AppError::Conflict(
                        msg("server.device.interface.mac_exists").with("mac", mac),
                    ));
                }
                crate::helpers::ensure_mac_available(&mut *tx, mac, Some(port_id)).await?;
            }

            if kept_ids.contains(&port_id) {
                // 保留网口原地更新：id/created_at 与二层运行属性（port_type、
                // status、speed、native_vlan_id）由端口模态框与 SNMP 维护，不在此覆盖
                sqlx::query(
                    r"UPDATE device_interfaces SET
                         nic_id = $1, name = $2, physical_type = $3, interface_role = $4,
                         mac_address = $5, vlan_id = $6, description = $7, sort_order = $8,
                         device_managed = TRUE, updated_at = $9
                       WHERE id = $10",
                )
                .bind(card_id)
                .bind(&port.name)
                .bind(physical_type)
                .bind(interface_role)
                .bind(&mac_address)
                .bind(port.vlan_id)
                .bind(&port.description)
                .bind(port_idx as i32)
                .bind(now)
                .bind(port_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    // 表达式唯一索引撞车说明 MAC 已被占用；其余唯一冲突为同设备重名
                    if let sqlx::Error::Database(ref db_err) = e
                        && db_err.is_unique_violation()
                    {
                        if db_err.constraint() == Some(crate::helpers::MAC_UNIQUE_INDEX) {
                            return AppError::Conflict(
                                msg("server.device.interface.mac_exists")
                                    .with("mac", mac_address.clone().unwrap_or_default()),
                            );
                        }
                        return AppError::Conflict(msg("server.device.interface.name_exists"));
                    }
                    AppError::from(e)
                })?;
            } else {
                // 新网口：二层属性无存量可继承，用默认值
                sqlx::query(
                    r"INSERT INTO device_interfaces (id, device_id, nic_id, name, physical_type, interface_role, mac_address, vlan_id, description, sort_order, port_type, status, speed, native_vlan_id, device_managed, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'access', 'up', NULL, NULL, TRUE, $11, $12)",
                )
                .bind(port_id)
                .bind(device_id)
                .bind(card_id)
                .bind(&port.name)
                .bind(physical_type)
                .bind(interface_role)
                .bind(&mac_address)
                .bind(port.vlan_id)
                .bind(&port.description)
                .bind(port_idx as i32)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    if let sqlx::Error::Database(ref db_err) = e
                        && db_err.is_unique_violation()
                    {
                        if db_err.constraint() == Some(crate::helpers::MAC_UNIQUE_INDEX) {
                            return AppError::Conflict(
                                msg("server.device.interface.mac_exists")
                                    .with("mac", mac_address.clone().unwrap_or_default()),
                            );
                        }
                        return AppError::Conflict(msg("server.device.interface.name_exists"));
                    }
                    AppError::from(e)
                })?;
            }

            // 本网口的存量 IP 基准（按地址差量）：命中者沿用原行（保留
            // status/last_seen/created_at/id），未命中者删除，新地址插入
            let mut port_existing: Vec<(Uuid, String, Uuid)> =
                existing_ips.remove(&port_id).unwrap_or_default();
            let mut seen_ips: HashSet<String> = HashSet::new();

            for ip in &port.ips {
                ip.validate()?;
                let ip_text = ip.ip_address.trim().to_string();
                let ip_key = foims_common::net::parse_inet(&ip_text)
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|| ip_text.clone());
                if !seen_ips.insert(ip_key) {
                    return Err(AppError::Conflict(
                        msg("server.ip.already_exists").with("ip", &ip_text),
                    ));
                }

                let subnet_id: Uuid = match ip.subnet_id {
                    Some(nid) => {
                        // 显式指定：子网必须绑定在设备所在房间，且地址必须落在
                        // 该子网同族 CIDR 内（否则 IP 与子网自相矛盾）
                        crate::helpers::validate_network_in_room(&mut *tx, room_id, Some(nid))
                            .await?;
                        crate::helpers::validate_ip_in_subnet(&mut *tx, nid, &ip_text).await?;
                        nid
                    }
                    // 未指定时按房间绑定网段探测；IP 必须归属子网
                    //（interface_ips.subnet_id NOT NULL），探测不中直接报错回滚
                    None => crate::helpers::auto_subnet_in_room(&mut *tx, room_id, &ip_text)
                        .await?
                        .ok_or_else(|| {
                            AppError::Validation(
                                msg("server.ip.network_required").with("ip", &ip_text),
                            )
                        })?,
                };

                if let Some(idx) = port_existing
                    .iter()
                    .position(|(_, addr, _)| crate::helpers::same_ip(addr, &ip_text))
                {
                    let (ip_id, _, current_subnet) = port_existing.remove(idx);
                    if current_subnet != subnet_id {
                        sqlx::query(
                            "UPDATE interface_ips SET subnet_id = $1, updated_at = $2 WHERE id = $3",
                        )
                        .bind(subnet_id)
                        .bind(now)
                        .bind(ip_id)
                        .execute(&mut *tx)
                        .await?;
                    }
                    continue;
                }

                // 新地址：全局唯一预检（并发窗口由 ip_address 唯一索引兜底）
                let occupied: Option<Uuid> = sqlx::query_scalar(
                    "SELECT id FROM interface_ips WHERE ip_address = CAST($1 AS INET)",
                )
                .bind(&ip_text)
                .fetch_optional(&mut *tx)
                .await?;
                if occupied.is_some() {
                    return Err(AppError::Conflict(
                        msg("server.ip.already_exists").with("ip", &ip_text),
                    ));
                }

                let ip_version = detect_ip_version(&ip_text)?;
                sqlx::query(
                    "INSERT INTO interface_ips (id, device_interface_id, subnet_id, ip_address, ip_version, status, last_seen, created_at, updated_at)
                     VALUES ($1, $2, $3, CAST($4 AS INET), $5, $6, $7, $8, $9)",
                )
                .bind(Uuid::new_v4())
                .bind(port_id)
                .bind(subnet_id)
                .bind(&ip_text)
                .bind(ip_version)
                .bind("active")
                .bind(now)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    // 与其他并发路径（单条创建/自动分配/导入）之间的判重窗口
                    // 由 interface_ips.ip_address 唯一索引兜底，映射为 409
                    if let sqlx::Error::Database(ref db_err) = e
                        && db_err.is_unique_violation()
                    {
                        return AppError::Conflict(
                            msg("server.ip.already_exists").with("ip", &ip_text),
                        );
                    }
                    AppError::from(e)
                })?;
            }

            // 表单中已移除的地址：直接删除（不做跨网口保留）
            for (ip_id, _, _) in port_existing {
                sqlx::query("DELETE FROM interface_ips WHERE id = $1")
                    .bind(ip_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }

    // 管理地址可能随本次差量同步改变：同事务刷新设备在主机监控（SNMP
    // 来源）中的 ip，轮询侧不再回写该字段
    super::host_monitor::sync_snmp_host_ip(tx, device_id).await?;

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
        sqlx::query_as::<_, IpDetail>(
            r"SELECT
                m.id, m.device_interface_id, di.device_id, m.subnet_id,
                nc.network_region_id AS network_region_id,
                nc.name AS network_name,
                nr.name AS network_region,
                host(m.ip_address) as ip_address,
                m.ip_version, di.mac_address AS mac_address,
                m.status, m.last_seen, m.created_at::TIMESTAMPTZ, m.updated_at::TIMESTAMPTZ
              FROM interface_ips m
              JOIN device_interfaces di ON m.device_interface_id = di.id
              LEFT JOIN network_cidrs nc ON m.subnet_id = nc.id
              LEFT JOIN network_regions nr ON nc.network_region_id = nr.id
              WHERE di.device_id = $1
              ORDER BY m.ip_address",
        )
        .bind(device_id)
        .fetch_all(pool)
    );

    let cards = cards?;
    let ports = ports?;

    let mut ips_by_port: HashMap<Uuid, Vec<IpDetail>> = HashMap::new();
    for ip in ip_rows? {
        ips_by_port
            .entry(ip.device_interface_id)
            .or_default()
            .push(ip);
    }

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

/// GET /api/resources/devices/{id}/nics：获取设备的网卡配置（含嵌套网口与 IP，
/// 设备不存在 404）。
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
    Ok(foims_common::ok_json(
        serde_json::json!({ "device_type": device_type, "cards": cards }),
        "server.device.nic.fetched",
    ))
}

/// 空 cards 校验：网卡配置必须由调用方显式提交（创建/同步/更新均同规），
/// 不允许回退硬编码默认网卡/网口
fn validate_cards_not_empty(cards: &[NetworkCardSyncItem]) -> Result<(), AppError> {
    if cards.is_empty() {
        return Err(AppError::Validation(msg(
            "server.device.nic.cards_required",
        )));
    }
    Ok(())
}

/// 校验网卡形态是否在白名单内，越界返回 400。
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

/// 校验网口物理接口类型是否在白名单内，越界返回 400。
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

/// 校验网口角色是否在白名单内，越界返回 400。
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

    #[test]
    fn test_port_group_prefix_slash_names() {
        assert_eq!(port_group_prefix("xg1/0/0/1"), "xg1");
        assert_eq!(
            port_group_prefix("GigabitEthernet1/0/1"),
            "GigabitEthernet1"
        );
        assert_eq!(port_group_prefix("FortyGigE1/0/24"), "FortyGigE1");
    }

    #[test]
    fn test_port_group_prefix_plain_names() {
        assert_eq!(port_group_prefix("eth0"), "eth");
        assert_eq!(port_group_prefix("eth10"), "eth");
        assert_eq!(port_group_prefix("Vlan-interface"), "Vlan-interface");
        assert_eq!(port_group_prefix("123"), "123");
        assert_eq!(port_group_prefix(""), "");
        // 首段为空的病态输入走去尾数字路径，结果仅影响分组展示
        assert_eq!(port_group_prefix("/0/1"), "/0/");
    }

    #[test]
    fn test_validate_cards_not_empty() {
        let err = validate_cards_not_empty(&[])
            .err()
            .unwrap_or_else(|| panic!("空 cards 应被拒绝"));
        assert!(
            matches!(err, AppError::Validation(_)),
            "应返回 Validation 错误，实际: {err}"
        );

        // 非空配置（名称合法性由模型 validate 把关）直接放行
        let ok_item: NetworkCardSyncItem =
            serde_json::from_value(serde_json::json!({ "name": "网卡1" }))
                .unwrap_or_else(|e| panic!("合法网卡应反序列化成功: {e}"));
        assert!(validate_cards_not_empty(&[ok_item]).is_ok());
    }
}
