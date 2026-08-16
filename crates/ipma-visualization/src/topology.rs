//! 拓扑节点与连线管理。

use axum::extract::Path;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
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
    pub source_device_id: Uuid,
    pub target_device_id: Uuid,
    pub source_device_port_id: Option<Uuid>,
    pub target_device_port_id: Option<Uuid>,
    pub label: Option<String>,
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
    pub vendor: Option<String>,
    pub snmp_version: Option<String>,
    pub snmp_port: Option<i32>,
    pub location: Option<String>,
    pub workstation_name: Option<String>,
    pub room_name: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct TopologyConnectionWithPorts {
    pub id: Uuid,
    pub source_device_id: Uuid,
    pub target_device_id: Uuid,
    pub source_device_port_id: Option<Uuid>,
    pub target_device_port_id: Option<Uuid>,
    pub label: Option<String>,
    pub auto_discovered: bool,
    pub source_device_name: Option<String>,
    pub target_device_name: Option<String>,
    pub source_port_number: Option<String>,
    pub source_port_name: Option<String>,
    pub target_port_number: Option<String>,
    pub target_port_name: Option<String>,
    /// 信息点链：源设备的接口到目标设备之间依次经过的信息点
    /// 每项包含 id、name
    #[sqlx(json)]
    pub outlet_chain: Option<Vec<serde_json::Value>>,
}

// ==================== 处理函数 ====================

pub async fn get_topology_nodes(pool: &PgPool) -> Result<Response, VisualizationError> {
    let nodes = sqlx::query_as::<_, TopologyNodeWithDevice>(
        r"SELECT tn.id, tn.device_id, tn.x, tn.y, tn.width, tn.height,
                 d.name AS device_name, d.device_type, d.brand, d.model,
                 d.vendor, d.snmp_version, d.snmp_port, d.location,
                 w.name AS workstation_name,
                 r.name AS room_name,
                 NULL AS ip_address
          FROM topology_nodes tn
          JOIN devices d ON tn.device_id = d.id
          LEFT JOIN workstations w ON d.workstation_id = w.id
          LEFT JOIN rooms r ON w.room_id = r.id
          ORDER BY tn.created_at",
    )
    .fetch_all(pool)
    .await?;

    Ok(ok_json(nodes, "获取拓扑节点成功"))
}

pub async fn save_topology_nodes(
    pool: &PgPool,
    req: TopologyNodesRequest,
) -> Result<Response, VisualizationError> {
    if req.nodes.is_empty() {
        return Err(VisualizationError::Validation(
            "节点列表不能为空".to_string(),
        ));
    }

    let mut tx = pool.begin().await?;

    let device_ids: Vec<Uuid> = req.nodes.iter().map(|n| n.device_id).collect();
    let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE id = ANY($1)")
        .bind(&device_ids)
        .fetch_one(&mut *tx)
        .await?;

    if existing_count as usize != device_ids.len() {
        return Err(VisualizationError::Validation(
            "部分设备ID不存在".to_string(),
        ));
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

    Ok(ok_json((), "拓扑节点保存成功"))
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
        return Err(VisualizationError::NotFound("拓扑节点未找到".to_string()));
    }

    Ok(ok_json((), "拓扑节点删除成功"))
}

pub async fn get_topology_connections(pool: &PgPool) -> Result<Response, VisualizationError> {
    let connections = sqlx::query_as::<_, TopologyConnectionWithPorts>(
        r"SELECT tc.id, tc.source_device_id, tc.target_device_id,
                 tc.source_device_port_id, tc.target_device_port_id, tc.label, tc.auto_discovered,
                 sd.name AS source_device_name,
                 td.name AS target_device_name,
                 sp.port_number AS source_port_number,
                 sp.port_name AS source_port_name,
                 tp.port_number AS target_port_number,
                 tp.port_name AS target_port_name,
                 COALESCE(
                   (
                     SELECT json_agg(json_build_object('id', no.id, 'name', no.name))
                     FROM net_outlets no
                     WHERE no.id IN (
                       SELECT CASE WHEN cl1.a_endpoint_type = 'net_outlet' THEN cl1.a_endpoint_id ELSE cl1.b_endpoint_id END AS outlet_id
                       FROM cable_links cl1
                       JOIN device_interfaces di ON
                         (cl1.a_endpoint_type = 'device_interface' AND cl1.a_endpoint_id = di.id)
                         OR (cl1.b_endpoint_type = 'device_interface' AND cl1.b_endpoint_id = di.id)
                       WHERE di.device_id = tc.source_device_id
                     )
                     AND no.id IN (
                       SELECT CASE WHEN cl2.a_endpoint_type = 'net_outlet' THEN cl2.a_endpoint_id ELSE cl2.b_endpoint_id END AS outlet_id
                       FROM cable_links cl2
                       WHERE (cl2.a_endpoint_type = 'device_port' AND cl2.a_endpoint_id = tc.target_device_port_id)
                          OR (cl2.b_endpoint_type = 'device_port' AND cl2.b_endpoint_id = tc.target_device_port_id)
                     )
                   ),
                   '[]'::json
                 ) AS outlet_chain
          FROM topology_connections tc
          JOIN devices sd ON tc.source_device_id = sd.id
          JOIN devices td ON tc.target_device_id = td.id
          LEFT JOIN device_ports sp ON tc.source_device_port_id = sp.id
          LEFT JOIN device_ports tp ON tc.target_device_port_id = tp.id
          ORDER BY tc.created_at",
    )
    .fetch_all(pool)
    .await?;

    Ok(ok_json(connections, "获取拓扑连线成功"))
}

pub async fn create_topology_connection(
    pool: &PgPool,
    req: TopologyConnectionRequest,
) -> Result<Response, VisualizationError> {
    if req.source_device_id == req.target_device_id {
        return Err(VisualizationError::Validation("不允许自连接".to_string()));
    }

    let row = sqlx::query_as::<_, (Uuid,)>(
        r"INSERT INTO topology_connections (source_device_id, target_device_id, source_device_port_id, target_device_port_id, label)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(req.source_device_id)
    .bind(req.target_device_id)
    .bind(req.source_device_port_id)
    .bind(req.target_device_port_id)
    .bind(&req.label)
    .fetch_one(pool)
    .await?;

    Ok(ok_json(
        serde_json::json!({ "id": row.0 }),
        "拓扑连线创建成功",
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
        return Err(VisualizationError::NotFound("拓扑连线未找到".to_string()));
    }

    Ok(ok_json((), "拓扑连线删除成功"))
}

// ==================== 自动发现（基于 cable_links） ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct AutoDiscoverResult {
    pub added_nodes: usize,
    pub added_connections: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoveredDevicePair {
    pub source_device_id: Uuid,
    pub target_device_id: Uuid,
    pub source_device_port_id: Option<Uuid>,
    pub target_device_port_id: Option<Uuid>,
}

pub async fn auto_discover_all_topology(
    pool: &PgPool,
) -> Result<AutoDiscoverResult, VisualizationError> {
    let mut added_nodes = 0usize;
    let mut added_connections = 0usize;

    let pairs = discover_device_pairs_via_cable_links(pool).await?;

    let mut device_ids: Vec<Uuid> = Vec::new();
    for p in &pairs {
        if !device_ids.contains(&p.source_device_id) {
            device_ids.push(p.source_device_id);
        }
        if !device_ids.contains(&p.target_device_id) {
            device_ids.push(p.target_device_id);
        }
    }

    for device_id in &device_ids {
        if ensure_topology_node(pool, *device_id).await? {
            added_nodes += 1;
        }
    }

    for p in &pairs {
        if ensure_topology_connection(
            pool,
            p.source_device_id,
            p.target_device_id,
            p.source_device_port_id,
            p.target_device_port_id,
        )
        .await?
        {
            added_connections += 1;
        }
    }

    Ok(AutoDiscoverResult {
        added_nodes,
        added_connections,
    })
}

/// 扫描 cable_links 表，推导出设备间的拓扑连接关系
///
/// 处理两类典型链路：
/// 1. (device_port ↔ device_interface): 直接得到交换机设备 ↔ 接口所属设备
/// 2. (device_port ↔ net_outlet): 交换机设备 ↔ 使用该信息点的设备
async fn discover_device_pairs_via_cable_links(
    pool: &PgPool,
) -> Result<Vec<DiscoveredDevicePair>, VisualizationError> {
    let mut pairs: Vec<DiscoveredDevicePair> = Vec::new();

    let iface_rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, Uuid)>(
        r"SELECT DISTINCT
            sp.device_id AS switch_device_id,
            CASE WHEN cl.a_endpoint_type = 'device_port' THEN cl.a_endpoint_id ELSE cl.b_endpoint_id END AS device_port_id,
            di.device_id AS iface_device_id
          FROM cable_links cl
          JOIN device_ports sp ON
            (cl.a_endpoint_type = 'device_port' AND cl.a_endpoint_id = sp.id)
            OR (cl.b_endpoint_type = 'device_port' AND cl.b_endpoint_id = sp.id)
          JOIN device_interfaces di ON
            (cl.a_endpoint_type = 'device_interface' AND cl.a_endpoint_id = di.id)
            OR (cl.b_endpoint_type = 'device_interface' AND cl.b_endpoint_id = di.id)
          WHERE sp.device_id <> di.device_id",
    )
    .fetch_all(pool)
    .await?;

    for (switch_device_id, device_port_id, iface_device_id) in iface_rows {
        pairs.push(DiscoveredDevicePair {
            source_device_id: switch_device_id,
            source_device_port_id: device_port_id,
            target_device_id: iface_device_id,
            target_device_port_id: None,
        });
    }

    // 通过 cable_links 发现设备对：交换机端口 ↔ 信息点 ↔ 设备接口
    // 即一台设备的 device_interface 通过若干 net_outlet 连到某交换机的 device_port
    let outlet_rows = sqlx::query_as::<_, (Uuid, Option<Uuid>, Uuid)>(
        r"SELECT DISTINCT
            sp.device_id AS switch_device_id,
            CASE WHEN cl_sp.a_endpoint_type = 'device_port' THEN cl_sp.a_endpoint_id ELSE cl_sp.b_endpoint_id END AS device_port_id,
            dv.id AS device_id
          FROM cable_links cl_sp
          JOIN device_ports sp ON
            (cl_sp.a_endpoint_type = 'device_port' AND cl_sp.a_endpoint_id = sp.id)
            OR (cl_sp.b_endpoint_type = 'device_port' AND cl_sp.b_endpoint_id = sp.id)
          JOIN net_outlets no ON
            (cl_sp.a_endpoint_type = 'net_outlet' AND cl_sp.a_endpoint_id = no.id)
            OR (cl_sp.b_endpoint_type = 'net_outlet' AND cl_sp.b_endpoint_id = no.id)
          JOIN cable_links cl_di ON
            (cl_di.a_endpoint_type = 'net_outlet' AND cl_di.a_endpoint_id = no.id)
            OR (cl_di.b_endpoint_type = 'net_outlet' AND cl_di.b_endpoint_id = no.id)
          JOIN device_interfaces di ON
            (cl_di.a_endpoint_type = 'device_interface' AND cl_di.a_endpoint_id = di.id)
            OR (cl_di.b_endpoint_type = 'device_interface' AND cl_di.b_endpoint_id = di.id)
          JOIN devices dv ON dv.id = di.device_id
          WHERE sp.device_id <> dv.id",
    )
    .fetch_all(pool)
    .await?;

    for (switch_device_id, device_port_id, device_id) in outlet_rows {
        pairs.push(DiscoveredDevicePair {
            source_device_id: switch_device_id,
            source_device_port_id: device_port_id,
            target_device_id: device_id,
            target_device_port_id: None,
        });
    }

    Ok(pairs)
}

/// HTTP 处理函数：触发批量自动发现
pub async fn trigger_auto_discover(pool: &PgPool) -> Result<Response, VisualizationError> {
    let result = auto_discover_all_topology(pool).await?;
    Ok(ok_json(result, "自动发现完成"))
}

/// 确保拓扑节点存在，返回是否新增
async fn ensure_topology_node(pool: &PgPool, device_id: Uuid) -> Result<bool, VisualizationError> {
    let existing_count: i64 = sqlx::query_scalar(
        r"SELECT COUNT(*) FROM topology_connections
          WHERE (source_device_id = $1 OR target_device_id = $1) AND auto_discovered = true",
    )
    .bind(device_id)
    .fetch_one(pool)
    .await?;

    let x = 100 + (existing_count as i32) * 250;
    let y = if existing_count > 0 { 350 } else { 100 };

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

/// 确保拓扑连线存在，返回是否新增
async fn ensure_topology_connection(
    pool: &PgPool,
    source_device_id: Uuid,
    target_device_id: Uuid,
    source_device_port_id: Option<Uuid>,
    target_device_port_id: Option<Uuid>,
) -> Result<bool, VisualizationError> {
    let result = sqlx::query(
        r"INSERT INTO topology_connections (source_device_id, target_device_id, source_device_port_id, target_device_port_id, auto_discovered)
         VALUES ($1, $2, $3, $4, true)
         ON CONFLICT (source_device_id, target_device_id, source_device_port_id, target_device_port_id) DO NOTHING",
    )
    .bind(source_device_id)
    .bind(target_device_id)
    .bind(source_device_port_id)
    .bind(target_device_port_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
