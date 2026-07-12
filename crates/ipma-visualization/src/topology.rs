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
    pub auto_discovered: bool,
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
                 tc.source_port_id, tc.target_port_id, tc.label, tc.auto_discovered,
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

// ==================== 自动发现 ====================

#[derive(Debug, Serialize, Deserialize)]
pub struct AutoDiscoverResult {
    pub added_nodes: usize,
    pub added_connections: usize,
}

/// 发现的关联关系
struct DiscoveredLink {
    switch_device_id: Uuid,
    switch_port_id: Option<Uuid>,
    device_port_id: Option<Uuid>,
}

/// 为设备自动发现拓扑关联并添加到拓扑图
pub async fn auto_discover_device_topology(
    pool: &PgPool,
    device_id: Uuid,
) -> Result<AutoDiscoverResult, VisualizationError> {
    let mut added_nodes = 0usize;
    let mut added_connections = 0usize;

    // 查询设备的 net_outlet_id 和 device_port_id
    let row = sqlx::query_as::<_, (Option<Uuid>, Option<Uuid>)>(
        r"SELECT net_outlet_id, device_port_id FROM devices WHERE id = $1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?;

    let (net_outlet_id, device_port_id) = match row {
        Some(r) => r,
        None => {
            return Ok(AutoDiscoverResult {
                added_nodes,
                added_connections,
            });
        }
    };

    let mut links: Vec<DiscoveredLink> = Vec::new();

    // 通过 device_port_id 发现：端口所属的设备即为交换机
    if let Some(sp_id) = device_port_id {
        let switch_row =
            sqlx::query_as::<_, (Uuid,)>(r"SELECT device_id FROM device_ports WHERE id = $1")
                .bind(sp_id)
                .fetch_optional(pool)
                .await?;

        if let Some((switch_device_id,)) = switch_row
            && switch_device_id != device_id
        {
            let device_default_port = get_default_port_id(pool, device_id).await;
            links.push(DiscoveredLink {
                switch_device_id,
                switch_port_id: Some(sp_id),
                device_port_id: device_default_port,
            });
        }
    }

    // 通过 net_outlet_id 发现
    if let Some(outlet_id) = net_outlet_id {
        let outlet_row = sqlx::query_as::<_, (Option<Uuid>, Option<Uuid>)>(
            r"SELECT device_port_id, peer_net_outlet_id FROM net_outlets WHERE id = $1",
        )
        .bind(outlet_id)
        .fetch_optional(pool)
        .await?;

        if let Some((outlet_device_port_id, peer_outlet_id)) = outlet_row {
            // 通过信息点的 device_port_id 找交换机
            if let Some(outlet_dp_id) = outlet_device_port_id {
                let switch_row = sqlx::query_as::<_, (Uuid,)>(
                    r"SELECT device_id FROM device_ports WHERE id = $1",
                )
                .bind(outlet_dp_id)
                .fetch_optional(pool)
                .await?;

                if let Some((switch_device_id,)) = switch_row
                    && switch_device_id != device_id
                {
                    let device_default_port = get_default_port_id(pool, device_id).await;
                    links.push(DiscoveredLink {
                        switch_device_id,
                        switch_port_id: Some(outlet_dp_id),
                        device_port_id: device_default_port,
                    });
                }
            }

            // 通过 peer 信息点的 device_port_id 找交换机
            if let Some(peer_id) = peer_outlet_id {
                let peer_row = sqlx::query_as::<_, (Option<Uuid>,)>(
                    r"SELECT device_port_id FROM net_outlets WHERE id = $1",
                )
                .bind(peer_id)
                .fetch_optional(pool)
                .await?;

                if let Some((Some(peer_dp),)) = peer_row {
                    let switch_row = sqlx::query_as::<_, (Uuid,)>(
                        r"SELECT device_id FROM device_ports WHERE id = $1",
                    )
                    .bind(peer_dp)
                    .fetch_optional(pool)
                    .await?;

                    if let Some((switch_device_id,)) = switch_row
                        && switch_device_id != device_id
                    {
                        let device_default_port = get_default_port_id(pool, device_id).await;
                        links.push(DiscoveredLink {
                            switch_device_id,
                            switch_port_id: Some(peer_dp),
                            device_port_id: device_default_port,
                        });
                    }
                }
            }
        }
    }

    // 为每个发现的关联添加拓扑节点和连线
    for link in &links {
        // 添加设备的拓扑节点
        let device_added = ensure_topology_node(pool, device_id).await?;
        if device_added {
            added_nodes += 1;
        }

        // 添加交换机的拓扑节点
        let switch_added = ensure_topology_node(pool, link.switch_device_id).await?;
        if switch_added {
            added_nodes += 1;
        }

        // 添加拓扑连线（设备 → 交换机）
        let conn_added = ensure_topology_connection(
            pool,
            device_id,
            link.switch_device_id,
            link.device_port_id,
            link.switch_port_id,
        )
        .await?;
        if conn_added {
            added_connections += 1;
        }
    }

    Ok(AutoDiscoverResult {
        added_nodes,
        added_connections,
    })
}

/// 批量扫描所有设备，自动发现拓扑关联
pub async fn auto_discover_all_topology(
    pool: &PgPool,
) -> Result<AutoDiscoverResult, VisualizationError> {
    let device_ids: Vec<Uuid> = sqlx::query_as::<_, (Uuid,)>(
        r"SELECT id FROM devices WHERE net_outlet_id IS NOT NULL OR device_port_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(id,)| id)
    .collect();

    let mut total_nodes = 0usize;
    let mut total_connections = 0usize;

    for device_id in device_ids {
        let result = auto_discover_device_topology(pool, device_id).await?;
        total_nodes += result.added_nodes;
        total_connections += result.added_connections;
    }

    Ok(AutoDiscoverResult {
        added_nodes: total_nodes,
        added_connections: total_connections,
    })
}

/// HTTP 处理函数：触发批量自动发现
pub async fn trigger_auto_discover(pool: &PgPool) -> Result<HttpResponse, VisualizationError> {
    let result = auto_discover_all_topology(pool).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(result, "自动发现完成")))
}

/// 确保拓扑节点存在，返回是否新增
async fn ensure_topology_node(pool: &PgPool, device_id: Uuid) -> Result<bool, VisualizationError> {
    // 查询交换机已有的下挂数量，用于计算位置
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
    source_port_id: Option<Uuid>,
    target_port_id: Option<Uuid>,
) -> Result<bool, VisualizationError> {
    let result = sqlx::query(
        r"INSERT INTO topology_connections (source_device_id, target_device_id, source_port_id, target_port_id, auto_discovered)
         VALUES ($1, $2, $3, $4, true)
         ON CONFLICT (source_device_id, target_device_id, source_port_id, target_port_id) DO NOTHING",
    )
    .bind(source_device_id)
    .bind(target_device_id)
    .bind(source_port_id)
    .bind(target_port_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// 获取设备的默认端口ID
async fn get_default_port_id(pool: &PgPool, device_id: Uuid) -> Option<Uuid> {
    sqlx::query_as::<_, (Uuid,)>(
        r"SELECT id FROM device_ports WHERE device_id = $1 AND port_number = 'default' LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|(id,)| id)
}
