//! 设备可视化：拓扑节点与连线管理。
//!
//! 连线分三类：
//! - 派生物理连线：由 cable_links 实时推导（设备↔设备，途经信息点/
//!   配线架，支持多段线路），不落库，随线路数据自动同步；
//! - 手动物理连线：topology_connections 中 connection_type='physical'
//!   的示意连线（未录入线路时的草图）；
//! - 逻辑连线：connection_type='logical'，两端成员端口组记录于
//!   topology_connection_members（链路聚合，同一对设备仅允许一条）。

use axum::extract::Path;
use axum::response::Response;
use ipma_common::msg;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

use crate::layout::{VisualizationError, ok_json};

// ==================== 请求/响应模型 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct TopologyNodeItem {
    pub device_id: Uuid,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopologyNodesRequest {
    pub nodes: Vec<TopologyNodeItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopologyConnectionRequest {
    /// 连线类型：physical（默认，手动示意）/ logical（链路聚合）
    pub connection_type: Option<String>,
    pub source_device_id: Uuid,
    pub target_device_id: Uuid,
    /// 手动物理连线：可选的单侧端口
    pub source_device_port_id: Option<Uuid>,
    pub target_device_port_id: Option<Uuid>,
    pub label: Option<String>,
    /// 逻辑连线：源侧成员端口组（链路聚合）
    pub source_port_ids: Option<Vec<Uuid>>,
    /// 逻辑连线：目标侧成员端口组
    pub target_port_ids: Option<Vec<Uuid>>,
}

/// 派生物理连线的中间节点（信息点/配线架）。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TopologyHop {
    pub node_type: String,
    pub node_id: Uuid,
    pub node_label: Option<String>,
}

/// 派生物理连线的一段线路。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TopologyCableSegment {
    pub cable_id: Uuid,
    pub cable_label: Option<String>,
}

/// 逻辑连线的成员端口。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TopologyMemberPort {
    pub port_id: Uuid,
    pub port_number: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TopologyConnectionView {
    /// 派生物理连线为合成 id（cable:{md5}），存储连线为 UUID
    pub id: String,
    /// physical | logical
    pub connection_type: String,
    /// 是否由线路（cable_links）实时派生
    pub derived: bool,
    pub source_device_id: Uuid,
    pub target_device_id: Uuid,
    pub source_device_name: Option<String>,
    pub target_device_name: Option<String>,
    pub source_port_id: Option<Uuid>,
    pub target_port_id: Option<Uuid>,
    pub source_port_label: Option<String>,
    pub target_port_label: Option<String>,
    pub label: Option<String>,
    pub auto_discovered: bool,
    /// 物理派生：途经的中间节点（信息点/配线架）
    pub hops: Vec<TopologyHop>,
    /// 物理派生：逐段线路（段数 = hops 数 + 1）
    pub cables: Vec<TopologyCableSegment>,
    /// 逻辑连线：源/目标侧成员端口组
    pub source_members: Vec<TopologyMemberPort>,
    pub target_members: Vec<TopologyMemberPort>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct TopologyNodeWithDevice {
    pub id: Uuid,
    pub device_id: Uuid,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub device_name: Option<String>,
    pub device_type: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub seller: Option<String>,
    pub snmp_version: Option<String>,
    pub snmp_port: Option<i32>,
    pub location: Option<String>,
    pub workstation_name: Option<String>,
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub org_id: Option<Uuid>,
    pub org_name: Option<String>,
    pub cabinet_name: Option<String>,
    pub ip_address: Option<String>,
}

// ==================== 节点管理 ====================

pub async fn get_topology_nodes(pool: &PgPool) -> Result<Response, VisualizationError> {
    let nodes = sqlx::query_as::<_, TopologyNodeWithDevice>(
        r"SELECT tn.id, tn.device_id, tn.x, tn.y, tn.width, tn.height,
                 d.name AS device_name, d.device_type, d.brand, d.model,
                 d.seller, d.snmp_version, d.snmp_port, d.location,
                 w.name AS workstation_name,
                 r.id AS room_id, r.name AS room_name,
                 o.id AS org_id, o.name AS org_name,
                 c.name AS cabinet_name,
                 (SELECT dm.ip_address::TEXT FROM device_macs dm
                   WHERE dm.device_id = d.id
                   ORDER BY dm.updated_at DESC LIMIT 1) AS ip_address
          FROM topology_nodes tn
          JOIN devices d ON tn.device_id = d.id
          LEFT JOIN workstations w ON d.workstation_id = w.id
          LEFT JOIN rooms r ON d.room_id = r.id
          LEFT JOIN organizations o ON r.org_id = o.id
          LEFT JOIN positions p ON d.position_id = p.id
          LEFT JOIN cabinets c ON p.cabinet_id = c.id
          ORDER BY tn.created_at",
    )
    .fetch_all(pool)
    .await?;

    Ok(ok_json(
        nodes,
        "server.visualization.topology_nodes_retrieved",
    ))
}

pub async fn save_topology_nodes(
    pool: &PgPool,
    req: TopologyNodesRequest,
) -> Result<Response, VisualizationError> {
    if req.nodes.is_empty() {
        return Err(VisualizationError::Validation(msg(
            "server.visualization.nodes_empty",
        )));
    }

    let mut tx = pool.begin().await?;

    let device_ids: Vec<Uuid> = req.nodes.iter().map(|n| n.device_id).collect();
    let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE id = ANY($1)")
        .bind(&device_ids)
        .fetch_one(&mut *tx)
        .await?;

    if existing_count as usize != device_ids.len() {
        return Err(VisualizationError::Validation(msg(
            "server.visualization.device_ids_invalid",
        )));
    }

    for node in &req.nodes {
        sqlx::query(
            r"INSERT INTO topology_nodes (device_id, x, y, width, height)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (device_id)
             DO UPDATE SET
                 x = EXCLUDED.x,
                 y = EXCLUDED.y,
                 width = EXCLUDED.width,
                 height = EXCLUDED.height,
                 updated_at = NOW()",
        )
        .bind(node.device_id)
        .bind(node.x)
        .bind(node.y)
        .bind(node.width)
        .bind(node.height)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(ok_json((), "server.visualization.topology_nodes_saved"))
}

pub async fn delete_topology_node(
    pool: &PgPool,
    device_id: Path<Uuid>,
) -> Result<Response, VisualizationError> {
    let device_id = device_id.0;

    let result = sqlx::query("DELETE FROM topology_nodes WHERE device_id = $1")
        .bind(device_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(VisualizationError::NotFound(msg(
            "server.visualization.topology_node_not_found",
        )));
    }

    Ok(ok_json((), "server.visualization.topology_node_deleted"))
}

// ==================== 连线查询（派生 + 存储） ====================

/// 递归 CTE 的派生结果行：via_* 数组按跳展开。
#[derive(Debug, FromRow)]
struct DerivedConnectionRow {
    id: String,
    source_device_id: Uuid,
    source_device_name: Option<String>,
    source_port_id: Uuid,
    source_port_label: Option<String>,
    target_device_id: Uuid,
    target_device_name: Option<String>,
    target_port_id: Uuid,
    target_port_label: Option<String>,
    via_types: Vec<String>,
    via_ids: Vec<Uuid>,
    via_cables: Vec<Uuid>,
    via_cable_labels: Vec<Option<String>>,
}

/// 基于 cable_links 实时推导设备↔设备的物理连线（含多段线路路径）。
///
/// 从每个设备侧端点（交换机端口/设备物理接口）出发，穿过信息点与
/// 配线架中间节点（深度上限 8），到达另一台设备的端点即终止；
/// 同一对端点按最短路径去重，方向取字典序保证 A→B/B→A 不重复。
async fn derive_physical_connections(
    pool: &PgPool,
) -> Result<Vec<TopologyConnectionView>, VisualizationError> {
    let rows = sqlx::query_as::<_, DerivedConnectionRow>(
        r"WITH RECURSIVE
        edges AS (
            SELECT a_endpoint_type AS from_type, a_endpoint_id AS from_id,
                   b_endpoint_type AS to_type, b_endpoint_id AS to_id,
                   id AS cable_id, cable_label
            FROM cable_links
            UNION ALL
            SELECT b_endpoint_type, b_endpoint_id, a_endpoint_type, a_endpoint_id, id, cable_label
            FROM cable_links
        ),
        start_points AS (
            SELECT 'device_port'::VARCHAR AS ep_type, dp.id AS ep_id, dp.device_id,
                   dp.port_number AS ep_label
            FROM device_ports dp
            UNION ALL
            SELECT 'device_interface'::VARCHAR, di.id, di.device_id, di.name
            FROM device_interfaces di
            WHERE di.physical_type <> 'virtual'
        ),
        walk AS (
            SELECT sp.ep_type AS cur_type, sp.ep_id AS cur_id,
                   sp.device_id AS src_device_id, sp.ep_id AS src_ep_id, sp.ep_label AS src_label,
                   ARRAY[]::VARCHAR[] AS via_types, ARRAY[]::UUID[] AS via_ids,
                   ARRAY[]::UUID[] AS via_cables, ARRAY[]::VARCHAR[] AS via_cable_labels,
                   ARRAY[sp.ep_id]::UUID[] AS visited, 0 AS depth
            FROM start_points sp
            UNION ALL
            SELECT e.to_type, e.to_id,
                   w.src_device_id, w.src_ep_id, w.src_label,
                   CASE WHEN w.depth = 0 THEN w.via_types ELSE w.via_types || e.from_type END,
                   CASE WHEN w.depth = 0 THEN w.via_ids ELSE w.via_ids || e.from_id END,
                   w.via_cables || e.cable_id,
                   w.via_cable_labels || e.cable_label,
                   w.visited || e.to_id, w.depth + 1
            FROM walk w
            JOIN edges e ON e.from_type = w.cur_type AND e.from_id = w.cur_id
            WHERE w.depth < 8
              AND NOT (e.to_id = ANY(w.visited))
              AND (w.depth = 0 OR w.cur_type IN ('net_outlet', 'patch_panel'))
        ),
        reached AS (
            SELECT w.src_device_id, w.src_ep_id, w.src_label, w.depth,
                   w.cur_id AS tgt_ep_id,
                   w.via_types, w.via_ids, w.via_cables, w.via_cable_labels,
                   CASE w.cur_type
                     WHEN 'device_port' THEN dp.device_id
                     WHEN 'device_interface' THEN di.device_id
                   END AS tgt_device_id,
                   CASE w.cur_type
                     WHEN 'device_port' THEN dp.port_number
                     WHEN 'device_interface' THEN di.name
                   END AS tgt_label
            FROM walk w
            LEFT JOIN device_ports dp ON w.cur_type = 'device_port' AND dp.id = w.cur_id
            LEFT JOIN device_interfaces di ON w.cur_type = 'device_interface' AND di.id = w.cur_id
            WHERE w.depth > 0 AND w.cur_type IN ('device_port', 'device_interface')
        )
        SELECT DISTINCT ON (LEAST(r.src_ep_id, r.tgt_ep_id), GREATEST(r.src_ep_id, r.tgt_ep_id))
               'cable:' || md5(r.via_cables::text) AS id,
               r.src_device_id AS source_device_id, sd.name AS source_device_name,
               r.src_ep_id AS source_port_id, r.src_label AS source_port_label,
               r.tgt_device_id AS target_device_id, td.name AS target_device_name,
               r.tgt_ep_id AS target_port_id, r.tgt_label AS target_port_label,
               r.via_types, r.via_ids, r.via_cables, r.via_cable_labels
        FROM reached r
        JOIN devices sd ON sd.id = r.src_device_id
        JOIN devices td ON td.id = r.tgt_device_id
        WHERE r.tgt_device_id IS NOT NULL AND r.tgt_device_id <> r.src_device_id
        ORDER BY LEAST(r.src_ep_id, r.tgt_ep_id), GREATEST(r.src_ep_id, r.tgt_ep_id),
                 r.depth, r.src_ep_id",
    )
    .fetch_all(pool)
    .await?;

    // 批量解析中间节点名称（信息点/配线架）
    let mut outlet_ids: Vec<Uuid> = Vec::new();
    let mut panel_ids: Vec<Uuid> = Vec::new();
    for row in &rows {
        for (node_type, node_id) in row.via_types.iter().zip(row.via_ids.iter()) {
            match node_type.as_str() {
                "net_outlet" => outlet_ids.push(*node_id),
                "patch_panel" => panel_ids.push(*node_id),
                _ => {}
            }
        }
    }

    let mut label_map: HashMap<Uuid, String> = HashMap::new();
    if !outlet_ids.is_empty() {
        let names: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, name FROM net_outlets WHERE id = ANY($1)")
                .bind(&outlet_ids)
                .fetch_all(pool)
                .await?;
        label_map.extend(names);
    }
    if !panel_ids.is_empty() {
        let names: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT id, name FROM patch_panels WHERE id = ANY($1)")
                .bind(&panel_ids)
                .fetch_all(pool)
                .await?;
        label_map.extend(names);
    }

    let connections = rows
        .into_iter()
        .map(|row| {
            let hops = row
                .via_types
                .iter()
                .zip(row.via_ids.iter())
                .map(|(node_type, node_id)| TopologyHop {
                    node_type: node_type.clone(),
                    node_id: *node_id,
                    node_label: label_map.get(node_id).cloned(),
                })
                .collect();
            let cables = row
                .via_cables
                .iter()
                .zip(row.via_cable_labels.iter())
                .map(|(cable_id, cable_label)| TopologyCableSegment {
                    cable_id: *cable_id,
                    cable_label: cable_label.clone(),
                })
                .collect();
            TopologyConnectionView {
                id: row.id,
                connection_type: "physical".to_string(),
                derived: true,
                source_device_id: row.source_device_id,
                target_device_id: row.target_device_id,
                source_device_name: row.source_device_name,
                target_device_name: row.target_device_name,
                source_port_id: Some(row.source_port_id),
                target_port_id: Some(row.target_port_id),
                source_port_label: row.source_port_label,
                target_port_label: row.target_port_label,
                label: None,
                auto_discovered: true,
                hops,
                cables,
                source_members: Vec::new(),
                target_members: Vec::new(),
            }
        })
        .collect();

    Ok(connections)
}

/// 存储连线成员的临时解析结构。
#[derive(Debug, Deserialize)]
struct StoredMember {
    side: String,
    port_id: Uuid,
    port_number: Option<String>,
}

#[derive(Debug, FromRow)]
struct StoredConnectionRow {
    id: String,
    connection_type: String,
    source_device_id: Uuid,
    target_device_id: Uuid,
    source_device_port_id: Option<Uuid>,
    target_device_port_id: Option<Uuid>,
    label: Option<String>,
    auto_discovered: bool,
    source_device_name: Option<String>,
    target_device_name: Option<String>,
    source_port_label: Option<String>,
    target_port_label: Option<String>,
    members: Option<serde_json::Value>,
}

/// 读取存储连线：手动物理示意 + 逻辑（链路聚合，含成员端口组）。
async fn stored_connections(
    pool: &PgPool,
) -> Result<Vec<TopologyConnectionView>, VisualizationError> {
    let rows = sqlx::query_as::<_, StoredConnectionRow>(
        r"SELECT tc.id::TEXT AS id, tc.connection_type,
                 tc.source_device_id, tc.target_device_id,
                 tc.source_device_port_id, tc.target_device_port_id,
                 tc.label, tc.auto_discovered,
                 sd.name AS source_device_name, td.name AS target_device_name,
                 sp.port_number AS source_port_label, tp.port_number AS target_port_label,
                 COALESCE(
                   (SELECT json_agg(json_build_object(
                             'side', m.side, 'port_id', m.device_port_id,
                             'port_number', dp.port_number) ORDER BY dp.port_number)
                    FROM topology_connection_members m
                    JOIN device_ports dp ON dp.id = m.device_port_id
                    WHERE m.connection_id = tc.id),
                   '[]'::json
                 ) AS members
          FROM topology_connections tc
          JOIN devices sd ON sd.id = tc.source_device_id
          JOIN devices td ON td.id = tc.target_device_id
          LEFT JOIN device_ports sp ON sp.id = tc.source_device_port_id
          LEFT JOIN device_ports tp ON tp.id = tc.target_device_port_id
          ORDER BY tc.created_at",
    )
    .fetch_all(pool)
    .await?;

    let mut connections = Vec::with_capacity(rows.len());
    for row in rows {
        let mut source_members = Vec::new();
        let mut target_members = Vec::new();
        if let Some(serde_json::Value::Array(items)) = &row.members {
            for item in items {
                if let Ok(member) = serde_json::from_value::<StoredMember>(item.clone()) {
                    let entry = TopologyMemberPort {
                        port_id: member.port_id,
                        port_number: member.port_number,
                    };
                    if member.side == "source" {
                        source_members.push(entry);
                    } else {
                        target_members.push(entry);
                    }
                }
            }
        }

        connections.push(TopologyConnectionView {
            id: row.id,
            connection_type: row.connection_type,
            derived: false,
            source_device_id: row.source_device_id,
            target_device_id: row.target_device_id,
            source_device_name: row.source_device_name,
            target_device_name: row.target_device_name,
            source_port_id: row.source_device_port_id,
            target_port_id: row.target_device_port_id,
            source_port_label: row.source_port_label,
            target_port_label: row.target_port_label,
            label: row.label,
            auto_discovered: row.auto_discovered,
            hops: Vec::new(),
            cables: Vec::new(),
            source_members,
            target_members,
        });
    }

    Ok(connections)
}

pub async fn get_topology_connections(pool: &PgPool) -> Result<Response, VisualizationError> {
    let mut connections = derive_physical_connections(pool).await?;
    connections.extend(stored_connections(pool).await?);

    Ok(ok_json(
        connections,
        "server.visualization.topology_connections_retrieved",
    ))
}

// ==================== 连线创建/删除 ====================

/// 校验端口列表全部属于指定设备，返回是否合法。
async fn ports_belong_to_device(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    port_ids: &[Uuid],
    device_id: Uuid,
) -> Result<bool, VisualizationError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_ports WHERE id = ANY($1) AND device_id = $2",
    )
    .bind(port_ids)
    .bind(device_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(count as usize == port_ids.len())
}

pub async fn create_topology_connection(
    pool: &PgPool,
    req: TopologyConnectionRequest,
) -> Result<Response, VisualizationError> {
    let connection_type = req
        .connection_type
        .unwrap_or_else(|| "physical".to_string());
    if !matches!(connection_type.as_str(), "physical" | "logical") {
        return Err(VisualizationError::Validation(msg(
            "server.visualization.connection_type_invalid",
        )));
    }

    if req.source_device_id == req.target_device_id {
        return Err(VisualizationError::Validation(msg(
            "server.visualization.self_connection_forbidden",
        )));
    }

    if let Some(label) = &req.label
        && label.chars().count() > 100
    {
        return Err(VisualizationError::Validation(msg(
            "server.visualization.connection_label_too_long",
        )));
    }

    let mut tx = pool.begin().await?;

    // 设备存在性
    let device_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE id = ANY($1)")
        .bind(vec![req.source_device_id, req.target_device_id])
        .fetch_one(&mut *tx)
        .await?;
    if device_count != 2 {
        return Err(VisualizationError::Validation(msg(
            "server.visualization.device_ids_invalid",
        )));
    }

    let new_id: Uuid;

    if connection_type == "logical" {
        // 链路聚合：两端必须提供成员端口组
        let source_port_ids = req.source_port_ids.clone().unwrap_or_default();
        let target_port_ids = req.target_port_ids.clone().unwrap_or_default();
        if source_port_ids.is_empty() || target_port_ids.is_empty() {
            return Err(VisualizationError::Validation(msg(
                "server.visualization.logical_members_required",
            )));
        }
        if source_port_ids.len()
            != source_port_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
            || target_port_ids.len()
                != target_port_ids
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
        {
            return Err(VisualizationError::Validation(msg(
                "server.visualization.logical_members_duplicated",
            )));
        }

        // 端口归属校验
        if !ports_belong_to_device(&mut tx, &source_port_ids, req.source_device_id).await?
            || !ports_belong_to_device(&mut tx, &target_port_ids, req.target_device_id).await?
        {
            return Err(VisualizationError::Validation(msg(
                "server.visualization.connection_ports_invalid",
            )));
        }

        // 成员端口不得同时参与多条逻辑连接
        let mut all_members = source_port_ids.clone();
        all_members.extend(target_port_ids.iter().copied());
        let aggregated: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM topology_connection_members WHERE device_port_id = ANY($1)",
        )
        .bind(&all_members)
        .fetch_one(&mut *tx)
        .await?;
        if aggregated > 0 {
            return Err(VisualizationError::Conflict(msg(
                "server.visualization.member_port_conflict",
            )));
        }

        // 同一对设备仅允许一条逻辑连接
        let existing: i64 = sqlx::query_scalar(
            r"SELECT COUNT(*) FROM topology_connections
               WHERE connection_type = 'logical'
                 AND LEAST(source_device_id, target_device_id) = LEAST($1, $2)
                 AND GREATEST(source_device_id, target_device_id) = GREATEST($1, $2)",
        )
        .bind(req.source_device_id)
        .bind(req.target_device_id)
        .fetch_one(&mut *tx)
        .await?;
        if existing > 0 {
            return Err(VisualizationError::Conflict(msg(
                "server.visualization.logical_connection_exists",
            )));
        }

        let row = sqlx::query_as::<_, (Uuid,)>(
            r"INSERT INTO topology_connections
                 (source_device_id, target_device_id, label, connection_type)
             VALUES ($1, $2, $3, 'logical')
             RETURNING id",
        )
        .bind(req.source_device_id)
        .bind(req.target_device_id)
        .bind(&req.label)
        .fetch_one(&mut *tx)
        .await?;
        new_id = row.0;

        for (side, port_ids) in [("source", &source_port_ids), ("target", &target_port_ids)] {
            for port_id in port_ids {
                sqlx::query(
                    r"INSERT INTO topology_connection_members
                         (connection_id, device_id, device_port_id, side)
                     VALUES ($1, $2, $3, $4)",
                )
                .bind(new_id)
                .bind(if side == "source" {
                    req.source_device_id
                } else {
                    req.target_device_id
                })
                .bind(port_id)
                .bind(side)
                .execute(&mut *tx)
                .await?;
            }
        }
    } else {
        // 手动物理示意连线：可选端口必须归属对应设备
        for (port_id, device_id) in [
            (req.source_device_port_id, req.source_device_id),
            (req.target_device_port_id, req.target_device_id),
        ] {
            if let Some(port_id) = port_id {
                let ports = vec![port_id];
                if !ports_belong_to_device(&mut tx, &ports, device_id).await? {
                    return Err(VisualizationError::Validation(msg(
                        "server.visualization.connection_ports_invalid",
                    )));
                }
            }
        }

        // 去重：同设备对同端口组合（含 NULL 端口）已存在则拒绝
        let existing: i64 = sqlx::query_scalar(
            r"SELECT COUNT(*) FROM topology_connections
               WHERE connection_type = 'physical'
                 AND LEAST(source_device_id, target_device_id) = LEAST($1, $2)
                 AND GREATEST(source_device_id, target_device_id) = GREATEST($1, $2)
                 AND source_device_port_id IS NOT DISTINCT FROM $3
                 AND target_device_port_id IS NOT DISTINCT FROM $4",
        )
        .bind(req.source_device_id)
        .bind(req.target_device_id)
        .bind(req.source_device_port_id)
        .bind(req.target_device_port_id)
        .fetch_one(&mut *tx)
        .await?;
        if existing > 0 {
            return Err(VisualizationError::Conflict(msg(
                "server.visualization.connection_duplicate",
            )));
        }

        let row = sqlx::query_as::<_, (Uuid,)>(
            r"INSERT INTO topology_connections
                 (source_device_id, target_device_id, source_device_port_id,
                  target_device_port_id, label, connection_type)
             VALUES ($1, $2, $3, $4, $5, 'physical')
             RETURNING id",
        )
        .bind(req.source_device_id)
        .bind(req.target_device_id)
        .bind(req.source_device_port_id)
        .bind(req.target_device_port_id)
        .bind(&req.label)
        .fetch_one(&mut *tx)
        .await?;
        new_id = row.0;
    }

    tx.commit().await?;

    Ok(ok_json(
        serde_json::json!({ "id": new_id }),
        "server.visualization.topology_connection_created",
    ))
}

pub async fn delete_topology_connection(
    pool: &PgPool,
    id: Path<Uuid>,
) -> Result<Response, VisualizationError> {
    let id = id.0;

    let result = sqlx::query("DELETE FROM topology_connections WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(VisualizationError::NotFound(msg(
            "server.visualization.topology_connection_not_found",
        )));
    }

    Ok(ok_json(
        (),
        "server.visualization.topology_connection_deleted",
    ))
}

// ==================== 自动发现（基于线路数据确保节点） ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct AutoDiscoverResult {
    pub added_nodes: usize,
    pub discovered_connections: usize,
}

/// 自动发现：为设备模块中存在而布局中缺失的设备补齐拓扑节点，
/// 并基于线路数据派生物理连线。
///
/// 物理连线自 SQL 派生后无需落库，自动发现仅负责补节点；
/// 同时清理历史遗留的 auto_discovered 物理连线（迁移兜底）。
pub async fn auto_discover_all_topology(
    pool: &PgPool,
) -> Result<AutoDiscoverResult, VisualizationError> {
    let derived = derive_physical_connections(pool).await?;

    let mut device_ids: Vec<Uuid> = Vec::new();
    for conn in &derived {
        if !device_ids.contains(&conn.source_device_id) {
            device_ids.push(conn.source_device_id);
        }
        if !device_ids.contains(&conn.target_device_id) {
            device_ids.push(conn.target_device_id);
        }
    }

    // 设备模块中的全部设备均确保存在节点（已存在的由 ON CONFLICT 跳过）
    let all_devices: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM devices ORDER BY name")
        .fetch_all(pool)
        .await?;
    for device_id in all_devices {
        if !device_ids.contains(&device_id) {
            device_ids.push(device_id);
        }
    }

    let mut added_nodes = 0usize;
    for device_id in &device_ids {
        if ensure_topology_node(pool, *device_id).await? {
            added_nodes += 1;
        }
    }

    sqlx::query("DELETE FROM topology_connections WHERE auto_discovered = TRUE")
        .execute(pool)
        .await?;

    Ok(AutoDiscoverResult {
        added_nodes,
        discovered_connections: derived.len(),
    })
}

/// HTTP 处理函数：触发批量自动发现
pub async fn trigger_auto_discover(pool: &PgPool) -> Result<Response, VisualizationError> {
    let result = auto_discover_all_topology(pool).await?;
    Ok(ok_json(
        result,
        "server.visualization.auto_discover_completed",
    ))
}

/// 确保拓扑节点存在，返回是否新增（按现有节点总数网格排布初始坐标）
async fn ensure_topology_node(pool: &PgPool, device_id: Uuid) -> Result<bool, VisualizationError> {
    let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM topology_nodes")
        .fetch_one(pool)
        .await?;

    let x = 100 + (existing_count as i32 % 8) * 250;
    let y = 100 + (existing_count as i32 / 8) * 150;

    let result = sqlx::query(
        r"INSERT INTO topology_nodes (device_id, x, y, width, height)
         VALUES ($1, $2, $3, 200, 100)
         ON CONFLICT (device_id) DO NOTHING",
    )
    .bind(device_id)
    .bind(x)
    .bind(y)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
