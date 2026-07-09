use actix_web::{HttpResponse, web};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::PgPool;
use uuid::Uuid;

use crate::layout::{ApiResponse, VisualizationError};

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
    pub source_port_id: Option<Uuid>,
    pub target_port_id: Option<Uuid>,
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
    pub source_port_id: Option<Uuid>,
    pub target_port_id: Option<Uuid>,
    pub label: Option<String>,
    pub source_device_name: Option<String>,
    pub target_device_name: Option<String>,
    pub source_port_number: Option<String>,
    pub source_port_name: Option<String>,
    pub target_port_number: Option<String>,
    pub target_port_name: Option<String>,
}

// ==================== 处理函数 ====================

pub async fn get_topology_nodes(pool: &PgPool) -> Result<HttpResponse, VisualizationError> {
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

    Ok(HttpResponse::Ok().json(ApiResponse::success(nodes, "获取拓扑节点成功")))
}

pub async fn save_topology_nodes(
    pool: &PgPool,
    req: TopologyNodesRequest,
) -> Result<HttpResponse, VisualizationError> {
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

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "拓扑节点保存成功")))
}

pub async fn delete_topology_node(
    pool: &PgPool,
    device_id: web::Path<Uuid>,
) -> Result<HttpResponse, VisualizationError> {
    let device_id = device_id.into_inner();

    let result = sqlx::query("DELETE FROM topology_nodes WHERE device_id = $1")
        .bind(device_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(VisualizationError::NotFound("拓扑节点未找到".to_string()));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "拓扑节点删除成功")))
}

pub async fn get_topology_connections(pool: &PgPool) -> Result<HttpResponse, VisualizationError> {
    let connections = sqlx::query_as::<_, TopologyConnectionWithPorts>(
        r"SELECT tc.id, tc.source_device_id, tc.target_device_id,
                 tc.source_port_id, tc.target_port_id, tc.label,
                 sd.name AS source_device_name,
                 td.name AS target_device_name,
                 sp.port_number AS source_port_number,
                 sp.port_name AS source_port_name,
                 tp.port_number AS target_port_number,
                 tp.port_name AS target_port_name
          FROM topology_connections tc
          JOIN devices sd ON tc.source_device_id = sd.id
          JOIN devices td ON tc.target_device_id = td.id
          LEFT JOIN device_ports sp ON tc.source_port_id = sp.id
	         LEFT JOIN device_ports tp ON tc.target_port_id = tp.id
          ORDER BY tc.created_at",
    )
    .fetch_all(pool)
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(connections, "获取拓扑连线成功")))
}

pub async fn create_topology_connection(
    pool: &PgPool,
    req: TopologyConnectionRequest,
) -> Result<HttpResponse, VisualizationError> {
    if req.source_device_id == req.target_device_id {
        return Err(VisualizationError::Validation("不允许自连接".to_string()));
    }

    let row = sqlx::query_as::<_, (Uuid,)>(
        r"INSERT INTO topology_connections (source_device_id, target_device_id, source_port_id, target_port_id, label)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
    )
    .bind(req.source_device_id)
    .bind(req.target_device_id)
    .bind(req.source_port_id)
    .bind(req.target_port_id)
    .bind(&req.label)
    .fetch_one(pool)
    .await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({ "id": row.0 }),
        "拓扑连线创建成功",
    )))
}

pub async fn delete_topology_connection(
    pool: &PgPool,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, VisualizationError> {
    let id = id.into_inner();

    let result = sqlx::query("DELETE FROM topology_connections WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(VisualizationError::NotFound("拓扑连线未找到".to_string()));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "拓扑连线删除成功")))
}
