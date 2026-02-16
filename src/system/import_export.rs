use actix_web::{HttpResponse, web};
use futures_util::TryStreamExt;
use sqlx::Row;
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use zip::{ZipWriter, write::FileOptions};

use crate::db::DbPool;
use crate::models::{
    ApiResponse, IpManagerWithNames, Network, NetworkRegion, Room, Workstation, WorkstationPort,
};
use serde::{Deserialize, Serialize};

// 导出数据结构体
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportData {
    pub network_regions: Vec<NetworkRegion>,
    pub networks: Vec<Network>,
    pub rooms: Vec<Room>,
    pub workstations: Vec<Workstation>,
    pub workstation_ports: Vec<WorkstationPort>,
    pub ip_managers: Vec<IpManagerWithNames>,
}

// 导入数据结构体
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportData {
    pub network_regions: Vec<NetworkRegion>,
    pub networks: Vec<Network>,
    pub rooms: Vec<Room>,
    pub workstations: Vec<Workstation>,
    pub workstation_ports: Vec<WorkstationPort>,
    pub ip_managers: Vec<IpManagerWithNames>,
}
use actix_web::Result;

// 导入JSON数据
pub async fn import_json(
    pool: web::Data<DbPool>,
    req: web::Json<ImportData>,
) -> Result<HttpResponse> {
    // 显式获取连接，用于批量导入操作
    let mut conn = pool.acquire().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取数据库连接失败: {}", e))
    })?;

    // 导入网络区域数据
    for network_region in &req.network_regions {
        if let Err(err) = sqlx::query(
            "INSERT INTO network_regions (id, name, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
             name = EXCLUDED.name,
             description = EXCLUDED.description,
             updated_at = EXCLUDED.updated_at",
        )
        .bind(network_region.id)
        .bind(&network_region.name)
        .bind(&network_region.description)
        .bind(network_region.created_at)
        .bind(network_region.updated_at)
        .execute(&mut *conn)
        .await
        {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "网络区域数据导入错误: {}",
                    err
                ))),
            );
        }
    }

    // 导入网络数据
    for network in &req.networks {
        if let Err(err) = sqlx::query(
            "INSERT INTO network_cidrs (id, name, network_region_id, ipv4_cidr, ipv6_cidr, ipv4_gateway, ipv6_gateway, ipv4_dns, ipv6_dns, description, created_at, updated_at)
             VALUES ($1, $2, $3, CAST($4 AS CIDR), CAST($5 AS CIDR), CAST($6 AS INET), CAST($7 AS INET), CAST($8 AS INET), CAST($9 AS INET), $10, $11, $12)
             ON CONFLICT (id) DO UPDATE SET
             name = EXCLUDED.name,
             network_region_id = EXCLUDED.network_region_id,
             ipv4_cidr = EXCLUDED.ipv4_cidr,
             ipv6_cidr = EXCLUDED.ipv6_cidr,
             ipv4_gateway = EXCLUDED.ipv4_gateway,
             ipv6_gateway = EXCLUDED.ipv6_gateway,
             ipv4_dns = EXCLUDED.ipv4_dns,
             ipv6_dns = EXCLUDED.ipv6_dns,
             description = EXCLUDED.description,
             updated_at = EXCLUDED.updated_at"
        )
        .bind(network.id)
        .bind(&network.name)
        .bind(network.network_region_id)
        .bind(&network.ipv4_cidr)
        .bind(&network.ipv6_cidr)
        .bind(&network.ipv4_gateway)
        .bind(&network.ipv6_gateway)
        .bind(&network.ipv4_dns)
        .bind(&network.ipv6_dns)
        .bind(&network.description)
        .bind(network.created_at)
        .bind(network.updated_at)
        .execute(&mut *conn)
        .await
        {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("网络数据导入错误: {}", err))));
        }
    }

    // 导入房间数据
    for room in &req.rooms {
        if let Err(err) = sqlx::query(
            "INSERT INTO rooms (id, name, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE
             SET name = EXCLUDED.name,
             description = EXCLUDED.description,
             updated_at = EXCLUDED.updated_at",
        )
        .bind(room.id)
        .bind(&room.name)
        .bind(&room.description)
        .bind(room.created_at)
        .bind(room.updated_at)
        .execute(&mut *conn)
        .await
        {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "房间数据导入错误: {}",
                    err
                ))),
            );
        }
    }

    // 导入工位数据
    for workstation in &req.workstations {
        if let Err(err) = sqlx::query(
            "INSERT INTO workstations (id, name, room_id, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET
             name = EXCLUDED.name,
             room_id = EXCLUDED.room_id,
             description = EXCLUDED.description,
             updated_at = EXCLUDED.updated_at",
        )
        .bind(workstation.id)
        .bind(&workstation.name)
        .bind(workstation.room_id)
        .bind(&workstation.description)
        .bind(workstation.created_at)
        .bind(workstation.updated_at)
        .execute(&mut *conn)
        .await
        {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "工位数据导入错误: {}",
                    err
                ))),
            );
        }
    }

    // 导入工位-交换机端口关联数据
    for workstation_port in &req.workstation_ports {
        if let Err(err) = sqlx::query(
            "INSERT INTO workstation_ports (id, workstation_id, switch_port_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
             workstation_id = EXCLUDED.workstation_id,
             switch_port_id = EXCLUDED.switch_port_id,
             updated_at = EXCLUDED.updated_at"
        )
        .bind(workstation_port.id)
        .bind(workstation_port.workstation_id)
        .bind(workstation_port.switch_port_id)
        .bind(workstation_port.created_at)
        .bind(workstation_port.updated_at)
        .execute(&mut *conn)
        .await {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("工位-交换机端口关联数据导入错误: {}", err))));
        }
    }

    // 导入IP映射数据
    for mapping in &req.ip_managers {
        if let Err(err) = sqlx::query(
            "INSERT INTO ip_managers (id, workstation_id, position_id, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
             VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (id) DO UPDATE SET
             workstation_id = EXCLUDED.workstation_id,
             position_id = EXCLUDED.position_id,
             network_id = EXCLUDED.network_id,
             ip_address = EXCLUDED.ip_address,
             ip_version = EXCLUDED.ip_version,
             mac_address = EXCLUDED.mac_address,
             hostname = EXCLUDED.hostname,
             status = EXCLUDED.status,
             last_seen = EXCLUDED.last_seen,
             updated_at = EXCLUDED.updated_at"
        )
        .bind(mapping.id)
        .bind(mapping.workstation_id)
        .bind(mapping.position_id)
        .bind(mapping.network_id)
        .bind(&mapping.ip_address)
        .bind(&mapping.ip_version)
        .bind(&mapping.mac_address)
        .bind(&mapping.hostname)
        .bind(&mapping.status)
        .bind(mapping.last_seen)
        .bind(mapping.created_at)
        .bind(mapping.updated_at)
        .execute(&mut *conn)
        .await {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("IP映射数据导入错误: {}", err))));
        }
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "JSON数据导入成功")))
}

// 导出JSON数据
pub async fn export_json(
    pool: web::Data<DbPool>,
    type_param: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    // 显式获取连接，用于批量导出操作
    let mut conn = pool.acquire().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取数据库连接失败: {}", e))
    })?;

    // 获取导出类型参数
    let export_type = type_param.get("type").cloned().unwrap_or("all".to_string());

    // 查询数据
    let (network_regions, networks, rooms, workstations, workstation_ports, ip_managers) =
        match export_type.as_str() {
            "all" => {
                let network_regions = match sqlx::query_as::<_, NetworkRegion>(
                "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions"
            ).fetch_all(&mut *conn).await {
                Ok(network_regions) => network_regions,
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let networks = match sqlx::query(
                r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
                   FROM network_cidrs n 
                   JOIN network_regions nt ON n.network_region_id = nt.id"#
            ).fetch_all(&mut *conn).await {
                Ok(rows) => {
                    rows.into_iter().map(|row| Network {
                        id: row.get(0),
                        name: row.get(1),
                        network_region_id: row.get(2),
                        network_region: row.get(3),
                        ipv4_cidr: row.get(4),
                        ipv6_cidr: row.get(5),
                        ipv4_gateway: row.get(6),
                        ipv6_gateway: row.get(7),
                        ipv4_dns: row.get(8),
                        ipv6_dns: row.get(9),
                        description: row.get(10),
                        created_at: row.get(11),
                        updated_at: row.get(12),
                    }).collect()
                },
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let rooms = match sqlx::query(
                "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms"
            ).fetch_all(&mut *conn).await {
                Ok(rows) => {
                    rows.into_iter().map(|row| Room {
                        id: row.get(0),
                        name: row.get(1),
                        room_type: row.get(2),
                        description: row.get(3),
                        created_at: row.get(4),
                        updated_at: row.get(5),
                    }).collect()
                },
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let workstations = match sqlx::query_as::<_, Workstation>(
                r#"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at, w.updated_at FROM workstations w JOIN rooms r ON w.room_id = r.id"#
            ).fetch_all(&mut *conn).await {
                Ok(workstations) => workstations,
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let workstation_ports = match sqlx::query_as::<_, WorkstationPort>(
                r#"SELECT wp.id, wp.workstation_id, w.name as workstation_name, wp.switch_port_id, sp.port_number as switch_port_number, s.name as switch_name, wp.created_at::TIMESTAMPTZ, wp.updated_at::TIMESTAMPTZ FROM workstation_ports wp JOIN workstations w ON wp.workstation_id = w.id JOIN switch_ports sp ON wp.switch_port_id = sp.id JOIN switches s ON sp.switch_id = s.id"#
            ).fetch_all(&mut *conn).await {
                Ok(workstation_ports) => workstation_ports,
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let ip_managers = match sqlx::query(
                r#"SELECT im.id, im.workstation_id, im.position_id, im.switch_id, im.switch_port_id, im.device_type, im.network_id, w.name as workstation_name, cp.name as cabinet_position_name, n.name as network_name, nt.name as network_region, CAST(im.ip_address AS TEXT), im.ip_version, im.mac_address, im.hostname, im.status, im.last_seen::TIMESTAMPTZ, im.created_at::TIMESTAMPTZ, im.updated_at::TIMESTAMPTZ FROM ip_managers im LEFT JOIN workstations w ON im.workstation_id = w.id LEFT JOIN positions cp ON im.position_id = cp.id LEFT JOIN network_cidrs n ON im.network_id = n.id LEFT JOIN network_regions nt ON n.network_region_id = nt.id"#
            ).fetch_all(&mut *conn).await {
                Ok(rows) => {
                    rows.into_iter().map(|row| IpManagerWithNames {
                        id: row.get(0),
                        workstation_id: row.get(1),
                        position_id: row.get(2),
                        switch_id: row.get(3),
                        switch_port_id: row.get(4),
                        device_type: row.get(5),
                        device_name: None,
                        network_id: row.get(6),
                        workstation_name: row.get(7),
                        cabinet_position_name: row.get(8),
                        switch_name: None,
                        switch_port_number: None,
                        network_name: row.get(9),
                        network_region: row.get(10),
                        ip_address: row.get(11),
                        ip_version: row.get(12),
                        mac_address: row.get(13),
                        hostname: row.get(14),
                        status: row.get(15),
                        last_seen: row.get(16),
                        created_at: row.get(17),
                        updated_at: row.get(18),
                    }).collect()
                },
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                (
                    network_regions,
                    networks,
                    rooms,
                    workstations,
                    workstation_ports,
                    ip_managers,
                )
            }
            _ => {
                // 对于特定类型，只查询对应的数据，其他返回空列表
                let network_regions = if export_type == "network_regions" {
                    match sqlx::query_as::<_, NetworkRegion>(
                    "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions"
                ).fetch_all(&mut *conn).await {
                    Ok(network_regions) => network_regions,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let networks = if export_type == "networks" {
                    match sqlx::query_as::<_, Network>(
                    r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
                       FROM network_cidrs n 
                       JOIN network_regions nt ON n.network_region_id = nt.id"#
                ).fetch_all(&mut *conn).await {
                    Ok(networks) => networks,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let rooms = if export_type == "rooms" {
                    match sqlx::query(
                    "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms"
                ).fetch_all(&mut *conn).await {
                    Ok(rows) => {
                        rows.into_iter().map(|row| Room {
                            id: row.get(0),
                            name: row.get(1),
                            room_type: row.get(2),
                            description: row.get(3),
                            created_at: row.get(4),
                            updated_at: row.get(5),
                        }).collect()
                    },
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let workstations = if export_type == "workstations" {
                    match sqlx::query_as::<_, Workstation>(
                    r#"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ FROM workstations w JOIN rooms r ON w.room_id = r.id"#
                ).fetch_all(&mut *conn).await {
                    Ok(workstations) => workstations,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let workstation_ports = if export_type == "workstations"
                    || export_type == "workstation_ports"
                {
                    match sqlx::query_as::<_, WorkstationPort>(
                    r#"SELECT wp.id, wp.workstation_id, w.name as workstation_name, wp.switch_port_id, sp.port_number as switch_port_number, s.name as switch_name, wp.created_at::TIMESTAMPTZ, wp.updated_at::TIMESTAMPTZ FROM workstation_ports wp JOIN workstations w ON wp.workstation_id = w.id JOIN switch_ports sp ON wp.switch_port_id = sp.id JOIN switches s ON sp.switch_id = s.id"#
                ).fetch_all(&mut *conn).await {
                    Ok(workstation_ports) => workstation_ports,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let ip_managers = if export_type == "ip_managers" {
                    match sqlx::query(
                    r#"SELECT im.id, im.workstation_id, im.position_id, im.switch_id, im.switch_port_id, im.device_type, im.network_id, w.name as workstation_name, cp.name as cabinet_position_name, n.name as network_name, nt.name as network_region, CAST(im.ip_address AS TEXT), im.ip_version, im.mac_address, im.hostname, im.status, im.last_seen::TIMESTAMPTZ, im.created_at::TIMESTAMPTZ, im.updated_at::TIMESTAMPTZ FROM ip_managers im LEFT JOIN workstations w ON im.workstation_id = w.id LEFT JOIN positions cp ON im.position_id = cp.id LEFT JOIN network_cidrs n ON im.network_id = n.id LEFT JOIN network_regions nt ON n.network_region_id = nt.id"#
                ).fetch_all(&mut *conn).await {
                    Ok(rows) => {
                        rows.into_iter().map(|row| IpManagerWithNames {
                            id: row.get(0),
                            workstation_id: row.get(1),
                            position_id: row.get(2),
                            switch_id: row.get(3),
                            switch_port_id: row.get(4),
                            device_type: row.get(5),
                            device_name: None,
                            network_id: row.get(6),
                            workstation_name: row.get(7),
                            cabinet_position_name: row.get(8),
                            switch_name: None,
                            switch_port_number: None,
                            network_name: row.get(9),
                            network_region: row.get(10),
                            ip_address: row.get(11),
                            ip_version: row.get(12),
                            mac_address: row.get(13),
                            hostname: row.get(14),
                            status: row.get(15),
                            last_seen: row.get(16),
                            created_at: row.get(17),
                            updated_at: row.get(18),
                        }).collect()
                    },
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                (
                    network_regions,
                    networks,
                    rooms,
                    workstations,
                    workstation_ports,
                    ip_managers,
                )
            }
        };

    // 构造导出数据
    let export_data = ExportData {
        network_regions,
        networks,
        rooms,
        workstations,
        workstation_ports,
        ip_managers,
    };

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=ipma_export_{}_{}.json",
                export_type,
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            ),
        ))
        .json(export_data))
}

// 辅助函数：将时间格式化为字符串
fn format_time(time: &chrono::DateTime<chrono::Utc>) -> String {
    time.format("%Y-%m-%d %H:%M:%S").to_string()
}

// 辅助函数：将Option<String>转换为String
fn format_option(option: &Option<String>) -> String {
    option
        .as_ref()
        .map_or_else(|| "".to_string(), |s| s.clone())
}

// 导出CSV数据
pub async fn export_csv(
    pool: web::Data<DbPool>,
    type_param: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    // 显式获取连接，用于批量导出操作
    let mut conn = pool.acquire().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取数据库连接失败: {}", e))
    })?;

    // 获取导出类型参数
    let export_type = type_param.get("type").cloned().unwrap_or("all".to_string());

    // 查询数据
    let (network_regions, networks, rooms, workstations, workstation_ports, ip_managers) =
        match export_type.as_str() {
            "all" => {
                let network_regions = match sqlx::query_as::<_, NetworkRegion>(
                "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions"
            ).fetch_all(&mut *conn).await {
                Ok(network_regions) => network_regions,
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let networks = match sqlx::query(
                r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
                   FROM network_cidrs n 
                   JOIN network_regions nt ON n.network_region_id = nt.id"#
            ).fetch_all(&mut *conn).await {
                Ok(rows) => {
                    rows.into_iter().map(|row| Network {
                        id: row.get(0),
                        name: row.get(1),
                        network_region_id: row.get(2),
                        network_region: row.get(3),
                        ipv4_cidr: row.get(4),
                        ipv6_cidr: row.get(5),
                        ipv4_gateway: row.get(6),
                        ipv6_gateway: row.get(7),
                        ipv4_dns: row.get(8),
                        ipv6_dns: row.get(9),
                        description: row.get(10),
                        created_at: row.get(11),
                        updated_at: row.get(12),
                    }).collect()
                },
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let rooms = match sqlx::query(
                "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms"
            ).fetch_all(&mut *conn).await {
                Ok(rows) => {
                    rows.into_iter().map(|row| Room {
                        id: row.get(0),
                        name: row.get(1),
                        room_type: row.get(2),
                        description: row.get(3),
                        created_at: row.get(4),
                        updated_at: row.get(5),
                    }).collect()
                },
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let workstations = match sqlx::query_as::<_, Workstation>(
                r#"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ FROM workstations w JOIN rooms r ON w.room_id = r.id"#
            ).fetch_all(&mut *conn).await {
                Ok(workstations) => workstations,
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let workstation_ports = match sqlx::query_as::<_, WorkstationPort>(
                r#"SELECT wp.id, wp.workstation_id, w.name as workstation_name, wp.switch_port_id, sp.port_number as switch_port_number, s.name as switch_name, wp.created_at::TIMESTAMPTZ, wp.updated_at::TIMESTAMPTZ FROM workstation_ports wp JOIN workstations w ON wp.workstation_id = w.id JOIN switch_ports sp ON wp.switch_port_id = sp.id JOIN switches s ON sp.switch_id = s.id"#
            ).fetch_all(&mut *conn).await {
                Ok(workstation_ports) => workstation_ports,
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                let ip_managers = match sqlx::query(
                r#"SELECT im.id, im.workstation_id, im.position_id, im.switch_id, im.switch_port_id, im.device_type, im.network_id, w.name as workstation_name, cp.name as cabinet_position_name, n.name as network_name, nt.name as network_region, CAST(im.ip_address AS TEXT), im.ip_version, im.mac_address, im.hostname, im.status, im.last_seen::TIMESTAMPTZ, im.created_at::TIMESTAMPTZ, im.updated_at::TIMESTAMPTZ FROM ip_managers im LEFT JOIN workstations w ON im.workstation_id = w.id LEFT JOIN positions cp ON im.position_id = cp.id LEFT JOIN network_cidrs n ON im.network_id = n.id LEFT JOIN network_regions nt ON n.network_region_id = nt.id"#
            ).fetch_all(&mut *conn).await {
                Ok(rows) => {
                    rows.into_iter().map(|row| IpManagerWithNames {
                        id: row.get(0),
                        workstation_id: row.get(1),
                        position_id: row.get(2),
                        switch_id: row.get(3),
                        switch_port_id: row.get(4),
                        device_type: row.get(5),
                        device_name: None,
                        network_id: row.get(6),
                        workstation_name: row.get(7),
                        cabinet_position_name: row.get(8),
                        switch_name: None,
                        switch_port_number: None,
                        network_name: row.get(9),
                        network_region: row.get(10),
                        ip_address: row.get(11),
                        ip_version: row.get(12),
                        mac_address: row.get(13),
                        hostname: row.get(14),
                        status: row.get(15),
                        last_seen: row.get(16),
                        created_at: row.get(17),
                        updated_at: row.get(18),
                    }).collect()
                },
                Err(err) => {
                    return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                }
            };

                (
                    network_regions,
                    networks,
                    rooms,
                    workstations,
                    workstation_ports,
                    ip_managers,
                )
            }
            _ => {
                // 对于特定类型，只查询对应的数据，其他返回空列表
                let network_regions = if export_type == "network_regions" {
                    match sqlx::query_as::<_, NetworkRegion>(
                    "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions"
                ).fetch_all(&mut *conn).await {
                    Ok(network_regions) => network_regions,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let networks = if export_type == "networks" {
                    match sqlx::query(
                    r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
                       FROM network_cidrs n 
                       JOIN network_regions nt ON n.network_region_id = nt.id"#
                ).fetch_all(&mut *conn).await {
                    Ok(rows) => {
                        rows.into_iter().map(|row| Network {
                            id: row.get(0),
                            name: row.get(1),
                            network_region_id: row.get(2),
                            network_region: row.get(3),
                            ipv4_cidr: row.get(4),
                            ipv6_cidr: row.get(5),
                            ipv4_gateway: row.get(6),
                            ipv6_gateway: row.get(7),
                            ipv4_dns: row.get(8),
                            ipv6_dns: row.get(9),
                            description: row.get(10),
                            created_at: row.get(11),
                            updated_at: row.get(12),
                        }).collect()
                    },
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let rooms = if export_type == "rooms" {
                    match sqlx::query(
                    "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms"
                ).fetch_all(&mut *conn).await {
                    Ok(rows) => {
                        rows.into_iter().map(|row| Room {
                            id: row.get(0),
                            name: row.get(1),
                            room_type: row.get(2),
                            description: row.get(3),
                            created_at: row.get(4),
                            updated_at: row.get(5),
                        }).collect()
                    },
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let workstations = if export_type == "workstations" {
                    match sqlx::query_as::<_, Workstation>(
                    r#"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at::TIMESTAMPTZ, w.updated_at::TIMESTAMPTZ FROM workstations w JOIN rooms r ON w.room_id = r.id"#
                ).fetch_all(&mut *conn).await {
                    Ok(workstations) => workstations,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let workstation_ports = if export_type == "workstations"
                    || export_type == "workstation_ports"
                {
                    match sqlx::query_as::<_, WorkstationPort>(
                    r#"SELECT wp.id, wp.workstation_id, w.name as workstation_name, wp.switch_port_id, sp.port_number as switch_port_number, s.name as switch_name, wp.created_at::TIMESTAMPTZ, wp.updated_at::TIMESTAMPTZ FROM workstation_ports wp JOIN workstations w ON wp.workstation_id = w.id JOIN switch_ports sp ON wp.switch_port_id = sp.id JOIN switches s ON sp.switch_id = s.id"#
                ).fetch_all(&mut *conn).await {
                    Ok(workstation_ports) => workstation_ports,
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                let ip_managers = if export_type == "ip_managers" {
                    match sqlx::query(
                    r#"SELECT im.id, im.workstation_id, im.position_id, im.switch_id, im.switch_port_id, im.device_type, im.network_id, w.name as workstation_name, cp.name as cabinet_position_name, n.name as network_name, nt.name as network_region, CAST(im.ip_address AS TEXT), im.ip_version, im.mac_address, im.hostname, im.status, im.last_seen::TIMESTAMPTZ, im.created_at::TIMESTAMPTZ, im.updated_at::TIMESTAMPTZ FROM ip_managers im LEFT JOIN workstations w ON im.workstation_id = w.id LEFT JOIN positions cp ON im.position_id = cp.id LEFT JOIN network_cidrs n ON im.network_id = n.id LEFT JOIN network_regions nt ON n.network_region_id = nt.id"#
                ).fetch_all(&mut *conn).await {
                    Ok(rows) => {
                        rows.into_iter().map(|row| IpManagerWithNames {
                            id: row.get(0),
                            workstation_id: row.get(1),
                            position_id: row.get(2),
                            switch_id: row.get(3),
                            switch_port_id: row.get(4),
                            device_type: row.get(5),
                            device_name: None,
                            network_id: row.get(6),
                            workstation_name: row.get(7),
                            cabinet_position_name: row.get(8),
                            switch_name: None,
                            switch_port_number: None,
                            network_name: row.get(9),
                            network_region: row.get(10),
                            ip_address: row.get(11),
                            ip_version: row.get(12),
                            mac_address: row.get(13),
                            hostname: row.get(14),
                            status: row.get(15),
                            last_seen: row.get(16),
                            created_at: row.get(17),
                            updated_at: row.get(18),
                        }).collect()
                    },
                    Err(err) => {
                        return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("数据库查询错误: {}", err))));
                    }
                }
                } else {
                    Vec::new()
                };

                (
                    network_regions,
                    networks,
                    rooms,
                    workstations,
                    workstation_ports,
                    ip_managers,
                )
            }
        };

    // 创建CSV数据
    let mut csv_data = Vec::new();

    // UTF-8 BOM标记
    let utf8_bom = &[0xEF, 0xBB, 0xBF];

    // 网络区域CSV
    if export_type == "all" || export_type == "network_regions" {
        let mut network_regions_csv = Vec::new();
        // 写入UTF-8 BOM
        network_regions_csv.extend_from_slice(utf8_bom);
        // 写入表头
        network_regions_csv.extend_from_slice("ID,名称,描述,创建时间,更新时间\n".as_bytes());
        // 写入数据
        for region in &network_regions {
            let line = format!(
                "{},{},{},{},{}\n",
                region.id,
                region.name,
                format_option(&region.description),
                format_time(&region.created_at),
                format_time(&region.updated_at)
            );
            network_regions_csv.extend_from_slice(line.as_bytes());
        }
        csv_data.push(("network_regions.csv", network_regions_csv));
    }

    // 网络CSV
    if export_type == "all" || export_type == "networks" {
        let mut networks_csv = Vec::new();
        // 写入UTF-8 BOM
        networks_csv.extend_from_slice(utf8_bom);
        // 写入表头
        networks_csv.extend_from_slice("ID,名称,网络区域,IPv4 CIDR,IPv6 CIDR,IPv4 网关,IPv6 网关,IPv4 DNS,IPv6 DNS,描述,创建时间,更新时间\n".as_bytes());
        // 写入数据
        for network in &networks {
            let line = format!(
                "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                network.id,
                network.name,
                network.network_region,
                format_option(&network.ipv4_cidr),
                format_option(&network.ipv6_cidr),
                format_option(&network.ipv4_gateway),
                format_option(&network.ipv6_gateway),
                format_option(&network.ipv4_dns),
                format_option(&network.ipv6_dns),
                format_option(&network.description),
                format_time(&network.created_at),
                format_time(&network.updated_at)
            );
            networks_csv.extend_from_slice(line.as_bytes());
        }
        csv_data.push(("networks.csv", networks_csv));
    }

    // 房间CSV
    if export_type == "all" || export_type == "rooms" {
        let mut rooms_csv = Vec::new();
        // 写入UTF-8 BOM
        rooms_csv.extend_from_slice(utf8_bom);
        // 写入表头
        rooms_csv.extend_from_slice("ID,名称,描述,创建时间,更新时间\n".as_bytes());
        // 写入数据
        for room in &rooms {
            let line = format!(
                "{},{},{},{},{}\n",
                room.id,
                room.name,
                format_option(&room.description),
                format_time(&room.created_at),
                format_time(&room.updated_at)
            );
            rooms_csv.extend_from_slice(line.as_bytes());
        }
        csv_data.push(("rooms.csv", rooms_csv));
    }

    // 工位CSV
    if export_type == "all" || export_type == "workstations" {
        let mut workstations_csv = Vec::new();
        // 写入UTF-8 BOM
        workstations_csv.extend_from_slice(utf8_bom);
        // 写入表头
        workstations_csv
            .extend_from_slice("ID,名称,房间,负责人,描述,创建时间,更新时间\n".as_bytes());
        // 写入数据
        for ws in &workstations {
            let line = format!(
                "{},{},{},{},{},{},{}\n",
                ws.id,
                ws.name,
                ws.room_name.as_deref().unwrap_or(""),
                format_option(&ws.manager),
                format_option(&ws.description),
                format_time(&ws.created_at),
                format_time(&ws.updated_at)
            );
            workstations_csv.extend_from_slice(line.as_bytes());
        }
        csv_data.push(("workstations.csv", workstations_csv));
    }

    // 工位端口CSV
    if export_type == "all" || export_type == "workstation_ports" {
        let mut workstation_ports_csv = Vec::new();
        // 写入UTF-8 BOM
        workstation_ports_csv.extend_from_slice(utf8_bom);
        // 写入表头
        workstation_ports_csv
            .extend_from_slice("ID,工位,交换机,端口号,创建时间,更新时间\n".as_bytes());
        // 写入数据
        for port in &workstation_ports {
            let line = format!(
                "{},{},{},{},{},{}\n",
                port.id,
                port.workstation_name.as_deref().unwrap_or(""),
                port.switch_name.as_deref().unwrap_or(""),
                port.switch_port_number.as_deref().unwrap_or(""),
                format_time(&port.created_at),
                format_time(&port.updated_at)
            );
            workstation_ports_csv.extend_from_slice(line.as_bytes());
        }
        csv_data.push(("workstation_ports.csv", workstation_ports_csv));
    }

    // IP管理CSV
    if export_type == "all" || export_type == "ip_managers" {
        let mut ip_managers_csv = Vec::new();
        // 写入UTF-8 BOM
        ip_managers_csv.extend_from_slice(utf8_bom);
        // 写入表头
        ip_managers_csv.extend_from_slice(
            "ID,工位,机位,网络,IP地址,IP版本,MAC地址,主机名,状态,最后在线,创建时间,更新时间\n"
                .as_bytes(),
        );
        // 写入数据
        for ip_manager in &ip_managers {
            let line = format!(
                "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                ip_manager.id,
                ip_manager.workstation_name.as_deref().unwrap_or(""),
                ip_manager.cabinet_position_name.as_deref().unwrap_or(""),
                ip_manager.network_name,
                ip_manager.ip_address,
                ip_manager.ip_version,
                format_option(&ip_manager.mac_address),
                format_option(&ip_manager.hostname),
                ip_manager.status,
                format_time(&ip_manager.last_seen),
                format_time(&ip_manager.created_at),
                format_time(&ip_manager.updated_at)
            );
            ip_managers_csv.extend_from_slice(line.as_bytes());
        }
        csv_data.push(("ip_managers.csv", ip_managers_csv));
    }

    // 创建ZIP文件
    let mut buf = Cursor::new(Vec::new());
    let options = FileOptions::<'_, ()>::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);

    {
        let mut zip = ZipWriter::new(&mut buf);

        // 将CSV文件添加到ZIP
        for (filename, data) in csv_data {
            if let Err(err) = zip.start_file(filename, options) {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "创建ZIP文件失败: {}",
                        err
                    ))),
                );
            }
            if let Err(err) = zip.write_all(&data) {
                return Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "写入ZIP文件失败: {}",
                        err
                    ))),
                );
            }
        }

        if let Err(err) = zip.finish() {
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "完成ZIP文件失败: {}",
                    err
                ))),
            );
        }
    }

    // 返回ZIP文件
    Ok(HttpResponse::Ok()
        .content_type("application/zip")
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=ipma_export_{}_{}.zip",
                export_type,
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            ),
        ))
        .body(buf.into_inner()))
}

// 导入CSV数据
pub async fn import_csv(
    pool: web::Data<DbPool>,
    mut payload: actix_multipart::Multipart,
) -> Result<HttpResponse> {
    // 读取上传的文件
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("读取文件失败: {}", e)))?
    {
        if field.name() == Some("file") {
            let mut data = Vec::new();
            while let Some(chunk) = field.try_next().await.map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!("读取文件块失败: {}", e))
            })? {
                data.extend_from_slice(&chunk);
            }
            file_data = Some(data);
            break;
        }
    }

    let file_data =
        file_data.ok_or_else(|| actix_web::error::ErrorBadRequest("请选择要导入的CSV文件"))?;

    // 存储导入结果
    let mut import_results = Vec::new();

    // 显式获取连接，用于处理CSV导入
    let mut conn = pool.acquire().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取数据库连接失败: {}", e))
    })?;

    // 尝试解析为ZIP文件
    if let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(file_data.clone())) {
        // 处理ZIP文件中的每个CSV文件
        for i in 0..zip.len() {
            let mut file = zip.by_index(i).map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!("读取ZIP文件项失败: {}", e))
            })?;

            // 只处理CSV文件
            if file.name().ends_with(".csv") {
                let mut content = String::new();
                file.read_to_string(&mut content).map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("读取CSV文件失败: {}", e))
                })?;

                // 处理单个CSV文件
                process_csv_file(&mut conn, &content, &mut import_results)
                    .await
                    .map_err(|e| {
                        actix_web::error::ErrorInternalServerError(format!(
                            "处理CSV文件失败: {}",
                            e
                        ))
                    })?;
            }
        }
    } else {
        // 不是ZIP文件，尝试直接处理为单个CSV文件
        let content = String::from_utf8(file_data).map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("解析CSV文件失败: {}", e))
        })?;

        process_csv_file(&mut conn, &content, &mut import_results)
            .await
            .map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!("处理CSV文件失败: {}", e))
            })?;
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<Vec<String>>::success(
        import_results,
        "CSV数据导入完成",
    )))
}

// 处理单个CSV文件
async fn process_csv_file(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    content: &str,
    import_results: &mut Vec<String>,
) -> Result<(), actix_web::Error> {
    // 检查CSV文件首行是否有标记
    let mut lines = content.lines();
    let first_line = lines.next().unwrap_or_default();

    // 解析首行标记
    let table_name = if first_line.starts_with("#table:") {
        first_line.trim_start_matches("#table:").trim()
    } else {
        // 如果没有标记，尝试从文件名推断
        "unknown"
    };

    // 重新组合CSV内容（跳过首行标记）
    let csv_content = if first_line.starts_with("#table:") {
        lines.collect::<Vec<_>>().join("\n")
    } else {
        content.to_string()
    };

    // 根据表名处理不同类型的数据
    match table_name {
        "network_regions" => process_network_regions_csv(conn, &csv_content, import_results).await,
        "networks" => process_networks_csv(conn, &csv_content, import_results).await,
        "rooms" => process_rooms_csv(conn, &csv_content, import_results).await,
        "workstations" => process_workstations_csv(conn, &csv_content, import_results).await,
        "workstation_ports" => {
            process_workstation_ports_csv(conn, &csv_content, import_results).await
        }
        "ip_managers" => process_ip_managers_csv(conn, &csv_content, import_results).await,
        _ => {
            import_results.push(format!("跳过未知表类型的CSV文件: {}", table_name));
            Ok(())
        }
    }
}

// 处理网络区域CSV
async fn process_network_regions_csv(
    conn: &mut sqlx::PgConnection,
    content: &str,
    import_results: &mut Vec<String>,
) -> Result<(), actix_web::Error> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    for record in rdr.records().flatten() {
        // 跳过表头
        if record.get(0).unwrap_or_default() == "ID" {
            continue;
        }

        // 生成新ID
        let id = uuid::Uuid::new_v4();
        let name = record.get(1).unwrap_or_default().trim();
        let description = record.get(2).unwrap_or_default().trim();

        // 检查是否已存在同名网络区域
        let existing =
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM network_regions WHERE name = $1")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("查询网络区域失败: {}", e))
                })?;

        if existing.is_none() {
            // 插入新网络区域
            sqlx::query("INSERT INTO network_regions (id, name, description, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                .bind(id)
                .bind(name)
                .bind(description)
                .execute(&mut *conn)
                .await
                .map_err(|e| actix_web::error::ErrorInternalServerError(format!("插入网络区域失败: {}", e)))?;

            import_results.push(format!("成功导入网络区域: {}", name));
        } else {
            import_results.push(format!("跳过网络区域（已存在）: {}", name));
        }
    }
    Ok(())
}

// 处理网络CSV
async fn process_networks_csv(
    conn: &mut sqlx::PgConnection,
    content: &str,
    import_results: &mut Vec<String>,
) -> Result<(), actix_web::Error> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    for record in rdr.records().flatten() {
        // 跳过表头
        if record.get(0).unwrap_or_default() == "ID" {
            continue;
        }

        // 生成新ID
        let id = uuid::Uuid::new_v4();
        let name = record.get(1).unwrap_or_default().trim();
        let network_region_name = record.get(2).unwrap_or_default().trim();
        let ipv4_cidr = record.get(4).unwrap_or_default().trim();
        let ipv6_cidr = record.get(5).unwrap_or_default().trim();
        let ipv4_gateway = record.get(6).unwrap_or_default().trim();
        let ipv6_gateway = record.get(7).unwrap_or_default().trim();
        let ipv4_dns = record.get(8).unwrap_or_default().trim();
        let ipv6_dns = record.get(9).unwrap_or_default().trim();
        let description = record.get(12).unwrap_or_default().trim();

        // 查找网络区域ID
        let network_region =
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM network_regions WHERE name = $1")
                .bind(network_region_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("查询网络区域失败: {}", e))
                })?;

        if let Some(region) = network_region {
            // 检查是否已存在同名网络
            let existing =
                sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM network_cidrs WHERE name = $1")
                    .bind(name)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(|e| {
                        actix_web::error::ErrorInternalServerError(format!("查询网络失败: {}", e))
                    })?;

            if existing.is_none() {
                // 插入新网络
                sqlx::query("INSERT INTO network_cidrs (id, name, network_region_id, ipv4_cidr, ipv6_cidr, ipv4_gateway, ipv6_gateway, ipv4_dns, ipv6_dns, description, created_at, updated_at) VALUES ($1, $2, $3, CAST($4 AS CIDR), CAST($5 AS CIDR), CAST($6 AS INET), CAST($7 AS INET), CAST($8 AS INET), CAST($9 AS INET), $10, NOW(), NOW())")
                    .bind(id)
                    .bind(name)
                    .bind(region)
                    .bind(ipv4_cidr)
                    .bind(ipv6_cidr)
                    .bind(ipv4_gateway)
                    .bind(ipv6_gateway)
                    .bind(ipv4_dns)
                    .bind(ipv6_dns)
                    .bind(description)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| actix_web::error::ErrorInternalServerError(format!("插入网络失败: {}", e)))?;

                import_results.push(format!("成功导入网络: {}", name));
            } else {
                import_results.push(format!("跳过网络（已存在）: {}", name));
            }
        } else {
            import_results.push(format!(
                "跳过网络（网络区域不存在）: {} - 网络区域: {}",
                name, network_region_name
            ));
        }
    }
    Ok(())
}

// 处理房间CSV
async fn process_rooms_csv(
    conn: &mut sqlx::PgConnection,
    content: &str,
    import_results: &mut Vec<String>,
) -> Result<(), actix_web::Error> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    for record in rdr.records().flatten() {
        // 跳过表头
        if record.get(0).unwrap_or_default() == "ID" {
            continue;
        }

        // 生成新ID
        let id = uuid::Uuid::new_v4();
        let name = record.get(1).unwrap_or_default().trim();
        let description = record.get(2).unwrap_or_default().trim();

        // 检查是否已存在同名房间
        let existing = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM rooms WHERE name = $1")
            .bind(name)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!("查询房间失败: {}", e))
            })?;

        if existing.is_none() {
            // 插入新房间
            sqlx::query("INSERT INTO rooms (id, name, description, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                .bind(id)
                .bind(name)
                .bind(description)
                .execute(&mut *conn)
                .await
                .map_err(|e| actix_web::error::ErrorInternalServerError(format!("插入房间失败: {}", e)))?;

            import_results.push(format!("成功导入房间: {}", name));
        } else {
            import_results.push(format!("跳过房间（已存在）: {}", name));
        }
    }
    Ok(())
}

// 处理工位CSV
async fn process_workstations_csv(
    conn: &mut sqlx::PgConnection,
    content: &str,
    import_results: &mut Vec<String>,
) -> Result<(), actix_web::Error> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    for record in rdr.records().flatten() {
        // 跳过表头
        if record.get(0).unwrap_or_default() == "ID" {
            continue;
        }

        // 生成新ID
        let id = uuid::Uuid::new_v4();
        let name = record.get(1).unwrap_or_default().trim();
        let room_name = record.get(2).unwrap_or_default().trim();
        let manager = record.get(3).unwrap_or_default().trim();
        let description = record.get(4).unwrap_or_default().trim();

        // 查找房间ID
        let room = sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM rooms WHERE name = $1")
            .bind(room_name)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!("查询房间失败: {}", e))
            })?;

        if let Some(room) = room {
            // 检查是否已存在同名工位
            let existing =
                sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM workstations WHERE name = $1")
                    .bind(name)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(|e| {
                        actix_web::error::ErrorInternalServerError(format!("查询工位失败: {}", e))
                    })?;

            if existing.is_none() {
                // 插入新工位
                sqlx::query("INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, NOW(), NOW())")
                    .bind(id)
                    .bind(name)
                    .bind(room)
                    .bind(manager)
                    .bind(description)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| actix_web::error::ErrorInternalServerError(format!("插入工位失败: {}", e)))?;

                import_results.push(format!("成功导入工位: {}", name));
            } else {
                import_results.push(format!("跳过工位（已存在）: {}", name));
            }
        } else {
            import_results.push(format!(
                "跳过工位（房间不存在）: {} - 房间: {}",
                name, room_name
            ));
        }
    }
    Ok(())
}

// 处理工位端口CSV
async fn process_workstation_ports_csv(
    conn: &mut sqlx::PgConnection,
    content: &str,
    import_results: &mut Vec<String>,
) -> Result<(), actix_web::Error> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    for record in rdr.records().flatten() {
        // 跳过表头
        if record.get(0).unwrap_or_default() == "ID" {
            continue;
        }

        // 生成新ID
        let id = uuid::Uuid::new_v4();
        let workstation_name = record.get(1).unwrap_or_default().trim();
        let switch_name = record.get(2).unwrap_or_default().trim();
        let port_number = record.get(3).unwrap_or_default().trim();

        // 查找工位ID
        let workstation =
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM workstations WHERE name = $1")
                .bind(workstation_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("查询工位失败: {}", e))
                })?;

        // 查找交换机端口ID
        let switch_port = sqlx::query_scalar::<_, uuid::Uuid>(
            "SELECT sp.id FROM switch_ports sp JOIN switches s ON sp.switch_id = s.id WHERE s.name = $1 AND sp.port_number = $2"
        )
        .bind(switch_name)
        .bind(port_number)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("查询交换机端口失败: {}", e)))?;

        if let (Some(workstation_id), Some(switch_port_id)) = (workstation, switch_port) {
            // 检查是否已存在相同的工位-端口关联
            let existing = sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT id FROM workstation_ports WHERE workstation_id = $1 AND switch_port_id = $2"
            )
            .bind(workstation_id)
            .bind(switch_port_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| actix_web::error::ErrorInternalServerError(format!("查询工位-端口关联失败: {}", e)))?;

            if existing.is_none() {
                // 插入新关联
                sqlx::query("INSERT INTO workstation_ports (id, workstation_id, switch_port_id, created_at, updated_at) VALUES ($1, $2, $3, NOW(), NOW())")
                    .bind(id)
                    .bind(workstation_id)
                    .bind(switch_port_id)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| actix_web::error::ErrorInternalServerError(format!("插入工位-端口关联失败: {}", e)))?;

                import_results.push(format!(
                    "成功导入工位-端口关联: {} - {}:{}",
                    workstation_name, switch_name, port_number
                ));
            } else {
                import_results.push(format!(
                    "跳过工位-端口关联（已存在）: {} - {}:{}",
                    workstation_name, switch_name, port_number
                ));
            }
        } else {
            import_results.push(format!(
                "跳过工位端口关联（工位或交换机不存在）: {} - {}:{}",
                workstation_name, switch_name, port_number
            ));
        }
    }
    Ok(())
}

// 处理IP管理CSV
async fn process_ip_managers_csv(
    conn: &mut sqlx::PgConnection,
    content: &str,
    import_results: &mut Vec<String>,
) -> Result<(), actix_web::Error> {
    let mut rdr = csv::Reader::from_reader(content.as_bytes());
    for record in rdr.records().flatten() {
        // 跳过表头
        if record.get(0).unwrap_or_default() == "ID" {
            continue;
        }

        // 生成新ID
        let id = uuid::Uuid::new_v4();
        let workstation_name = record.get(1).unwrap_or_default().trim();
        let cabinet_position_name = record.get(2).unwrap_or_default().trim();
        let network_name = record.get(3).unwrap_or_default().trim();
        let ip_address = record.get(4).unwrap_or_default().trim();
        let ip_version = record.get(5).unwrap_or_default().trim();
        let mac_address = record.get(6).unwrap_or_default().trim();
        let hostname = record.get(7).unwrap_or_default().trim();
        let status = record.get(8).unwrap_or_default().trim();

        // 查找工位ID
        let workstation = if !workstation_name.is_empty() {
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM workstations WHERE name = $1")
                .bind(workstation_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("查询工位失败: {}", e))
                })?
        } else {
            None
        };

        // 查找机位ID
        let cabinet_position = if !cabinet_position_name.is_empty() {
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM positions WHERE name = $1")
                .bind(cabinet_position_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("查询机位失败: {}", e))
                })?
        } else {
            None
        };

        // 查找网络ID
        let network =
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM network_cidrs WHERE name = $1")
                .bind(network_name)
                .fetch_optional(&mut *conn)
                .await
                .map_err(|e| {
                    actix_web::error::ErrorInternalServerError(format!("查询网络失败: {}", e))
                })?;

        if let Some(network) = network {
            // 检查是否已存在相同的IP地址
            let existing = sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT id FROM ip_managers WHERE ip_address = CAST($1 AS INET)",
            )
            .bind(ip_address)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| {
                actix_web::error::ErrorInternalServerError(format!("查询IP管理失败: {}", e))
            })?;

            if existing.is_none() {
                // 插入新IP管理记录
                sqlx::query("INSERT INTO ip_managers (id, workstation_id, position_id, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, NOW(), NOW(), NOW())")
                    .bind(id)
                    .bind(workstation)
                    .bind(cabinet_position)
                    .bind(network)
                    .bind(ip_address)
                    .bind(ip_version)
                    .bind(if mac_address.is_empty() { None } else { Some(mac_address) })
                    .bind(if hostname.is_empty() { None } else { Some(hostname) })
                    .bind(status)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| actix_web::error::ErrorInternalServerError(format!("插入IP管理记录失败: {}", e)))?;

                import_results.push(format!("成功导入IP管理记录: {}", ip_address));
            } else {
                import_results.push(format!("跳过IP管理记录（已存在）: {}", ip_address));
            }
        } else {
            import_results.push(format!(
                "跳过IP管理记录（网络不存在）: {} - 网络: {}",
                ip_address, network_name
            ));
        }
    }
    Ok(())
}

// 导出数据库
pub async fn export_database(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    // 显式获取连接，用于批量导出操作
    let mut conn = pool.acquire().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取数据库连接失败: {}", e))
    })?;

    // 导出所有数据
    let network_regions = sqlx::query_as::<_, NetworkRegion>(
        "SELECT id, name, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM network_regions"
    ).fetch_all(&mut *conn).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("导出网络区域失败: {}", e))
    })?;

    let networks = sqlx::query(
        r#"SELECT n.id, n.name, n.network_region_id, nt.name as network_region, n.ipv4_cidr::TEXT, n.ipv6_cidr::TEXT, n.ipv4_gateway::TEXT, n.ipv6_gateway::TEXT, n.ipv4_dns::TEXT, n.ipv6_dns::TEXT, n.description, n.created_at::TIMESTAMPTZ, n.updated_at::TIMESTAMPTZ 
           FROM network_cidrs n 
           JOIN network_regions nt ON n.network_region_id = nt.id"#
    ).fetch_all(&mut *conn).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("导出网络失败: {}", e))
    })?
    .into_iter().map(|row| Network {
        id: row.get(0),
        name: row.get(1),
        network_region_id: row.get(2),
        network_region: row.get(3),
        ipv4_cidr: row.get(4),
        ipv6_cidr: row.get(5),
        ipv4_gateway: row.get(6),
        ipv6_gateway: row.get(7),
        ipv4_dns: row.get(8),
        ipv6_dns: row.get(9),
        description: row.get(10),
        created_at: row.get(11),
        updated_at: row.get(12),
    }).collect();

    let rooms = sqlx::query(
        "SELECT id, name, room_type, description, created_at::TIMESTAMPTZ, updated_at::TIMESTAMPTZ FROM rooms"
    ).fetch_all(&mut *conn).await.map(|rows| {
        rows.into_iter().map(|row| Room {
            id: row.get(0),
            name: row.get(1),
            room_type: row.get(2),
            description: row.get(3),
            created_at: row.get(4),
            updated_at: row.get(5),
        }).collect()
    }).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("导出房间失败: {}", e))
    })?;

    let workstations = sqlx::query_as::<_, Workstation>(
        r#"SELECT w.id, w.name, w.room_id, r.name as room_name, w.manager, w.description, w.created_at, w.updated_at FROM workstations w JOIN rooms r ON w.room_id = r.id"#
    ).fetch_all(&mut *conn).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("导出工位失败: {}", e))
    })?;

    let workstation_ports = sqlx::query_as::<_, WorkstationPort>(
        r#"SELECT wp.id, wp.workstation_id, w.name as workstation_name, wp.switch_port_id, sp.port_number as switch_port_number, s.name as switch_name, wp.created_at::TIMESTAMPTZ, wp.updated_at::TIMESTAMPTZ FROM workstation_ports wp JOIN workstations w ON wp.workstation_id = w.id JOIN switch_ports sp ON wp.switch_port_id = sp.id JOIN switches s ON sp.switch_id = s.id"#
    ).fetch_all(&mut *conn).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("导出工位端口失败: {}", e))
    })?;

    let ip_managers = sqlx::query(
        r#"SELECT im.id, im.workstation_id, im.position_id, im.switch_id, im.switch_port_id, im.device_type, im.network_id, w.name as workstation_name, cp.name as cabinet_position_name, n.name as network_name, nt.name as network_region, CAST(im.ip_address AS TEXT), im.ip_version, im.mac_address, im.hostname, im.status, im.last_seen::TIMESTAMPTZ, im.created_at::TIMESTAMPTZ, im.updated_at::TIMESTAMPTZ FROM ip_managers im LEFT JOIN workstations w ON im.workstation_id = w.id LEFT JOIN positions cp ON im.position_id = cp.id LEFT JOIN network_cidrs n ON im.network_id = n.id LEFT JOIN network_regions nt ON n.network_region_id = nt.id"#
    ).fetch_all(&mut *conn).await.map(|rows| {
        rows.into_iter().map(|row| IpManagerWithNames {
            id: row.get(0),
            workstation_id: row.get(1),
            position_id: row.get(2),
            switch_id: row.get(3),
            switch_port_id: row.get(4),
            device_type: row.get(5),
            device_name: None,
            network_id: row.get(6),
            workstation_name: row.get(7),
            cabinet_position_name: row.get(8),
            switch_name: None,
            switch_port_number: None,
            network_name: row.get(9),
            network_region: row.get(10),
            ip_address: row.get(11),
            ip_version: row.get(12),
            mac_address: row.get(13),
            hostname: row.get(14),
            status: row.get(15),
            last_seen: row.get(16),
            created_at: row.get(17),
            updated_at: row.get(18),
        }).collect()
    }).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("导出IP管理失败: {}", e))
    })?;

    // 构造导出数据
    let export_data = ExportData {
        network_regions,
        networks,
        rooms,
        workstations,
        workstation_ports,
        ip_managers,
    };

    // 返回JSON数据
    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=ipma_database_export_{}.json",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            ),
        ))
        .json(export_data))
}

// 导入数据库
pub async fn import_database(
    pool: web::Data<DbPool>,
    req: web::Json<ImportData>,
) -> Result<HttpResponse> {
    // 显式获取连接，用于批量导入操作
    let mut conn = pool.acquire().await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取数据库连接失败: {}", e))
    })?;

    // 导入网络区域数据
    for network_region in &req.network_regions {
        sqlx::query(
            "INSERT INTO network_regions (id, name, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
             name = EXCLUDED.name,
             description = EXCLUDED.description,
             updated_at = EXCLUDED.updated_at",
        )
        .bind(network_region.id)
        .bind(&network_region.name)
        .bind(&network_region.description)
        .bind(network_region.created_at)
        .bind(network_region.updated_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("导入网络区域失败: {}", e))
        })?;
    }

    // 导入网络数据
    for network in &req.networks {
        sqlx::query(
            "INSERT INTO network_cidrs (id, name, network_region_id, ipv4_cidr, ipv6_cidr, ipv4_gateway, ipv6_gateway, ipv4_dns, ipv6_dns, description, created_at, updated_at)
             VALUES ($1, $2, $3, CAST($4 AS CIDR), CAST($5 AS CIDR), CAST($6 AS INET), CAST($7 AS INET), CAST($8 AS INET), CAST($9 AS INET), $10, $11, $12)
             ON CONFLICT (id) DO UPDATE SET
             name = EXCLUDED.name,
             network_region_id = EXCLUDED.network_region_id,
             ipv4_cidr = EXCLUDED.ipv4_cidr,
             ipv6_cidr = EXCLUDED.ipv6_cidr,
             ipv4_gateway = EXCLUDED.ipv4_gateway,
             ipv6_gateway = EXCLUDED.ipv6_gateway,
             ipv4_dns = EXCLUDED.ipv4_dns,
             ipv6_dns = EXCLUDED.ipv6_dns,
             description = EXCLUDED.description,
             updated_at = EXCLUDED.updated_at"
        )
        .bind(network.id)
        .bind(&network.name)
        .bind(network.network_region_id)
        .bind(&network.ipv4_cidr)
        .bind(&network.ipv6_cidr)
        .bind(&network.ipv4_gateway)
        .bind(&network.ipv6_gateway)
        .bind(&network.ipv4_dns)
        .bind(&network.ipv6_dns)
        .bind(&network.description)
        .bind(network.created_at)
        .bind(network.updated_at)
        .execute(&mut *conn)
        .await.map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("导入网络失败: {}", e))
        })?;
    }

    // 导入房间数据
    for room in &req.rooms {
        sqlx::query(
            "INSERT INTO rooms (id, name, room_type, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE
             SET name = EXCLUDED.name,
             room_type = EXCLUDED.room_type,
             description = EXCLUDED.description,
             updated_at = EXCLUDED.updated_at",
        )
        .bind(room.id)
        .bind(&room.name)
        .bind(&room.room_type)
        .bind(&room.description)
        .bind(room.created_at)
        .bind(room.updated_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(format!("导入房间失败: {}", e)))?;
    }

    // 导入工位数据
    for workstation in &req.workstations {
        sqlx::query(
            "INSERT INTO workstations (id, name, room_id, manager, description, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (id) DO UPDATE SET
             name = EXCLUDED.name,
             room_id = EXCLUDED.room_id,
             manager = EXCLUDED.manager,
             description = EXCLUDED.description,
             updated_at = EXCLUDED.updated_at",
        )
        .bind(workstation.id)
        .bind(&workstation.name)
        .bind(workstation.room_id)
        .bind(&workstation.manager)
        .bind(&workstation.description)
        .bind(workstation.created_at)
        .bind(workstation.updated_at)
        .execute(&mut *conn)
        .await.map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("导入工位失败: {}", e))
        })?;
    }

    // 导入工位-交换机端口关联数据
    for workstation_port in &req.workstation_ports {
        sqlx::query(
            "INSERT INTO workstation_ports (id, workstation_id, switch_port_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
             workstation_id = EXCLUDED.workstation_id,
             switch_port_id = EXCLUDED.switch_port_id,
             updated_at = EXCLUDED.updated_at"
        )
        .bind(workstation_port.id)
        .bind(workstation_port.workstation_id)
        .bind(workstation_port.switch_port_id)
        .bind(workstation_port.created_at)
        .bind(workstation_port.updated_at)
        .execute(&mut *conn)
        .await.map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("导入工位端口失败: {}", e))
        })?;
    }

    // 导入IP映射数据
    for mapping in &req.ip_managers {
        sqlx::query(
            "INSERT INTO ip_managers (id, workstation_id, position_id, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (id) DO UPDATE SET
             workstation_id = EXCLUDED.workstation_id,
             position_id = EXCLUDED.position_id,
             network_id = EXCLUDED.network_id,
             ip_address = EXCLUDED.ip_address,
             ip_version = EXCLUDED.ip_version,
             mac_address = EXCLUDED.mac_address,
             hostname = EXCLUDED.hostname,
             status = EXCLUDED.status,
             last_seen = EXCLUDED.last_seen,
             updated_at = EXCLUDED.updated_at"
        )
        .bind(mapping.id)
        .bind(mapping.workstation_id)
        .bind(mapping.position_id)
        .bind(mapping.network_id)
        .bind(&mapping.ip_address)
        .bind(&mapping.ip_version)
        .bind(&mapping.mac_address)
        .bind(&mapping.hostname)
        .bind(&mapping.status)
        .bind(mapping.last_seen)
        .bind(mapping.created_at)
        .bind(mapping.updated_at)
        .execute(&mut *conn)
        .await.map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("导入IP管理失败: {}", e))
        })?;
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "数据库导入成功")))
}

// 下载导入模板
pub async fn download_template(
    type_param: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    // 获取模板类型参数
    let template_type = type_param
        .get("type")
        .cloned()
        .unwrap_or("json".to_string());

    match template_type.as_str() {
        "json" => {
            // 创建当前时间
            let now = chrono::Utc::now();

            // 创建JSON模板
            let template = ExportData {
                network_regions: vec![NetworkRegion {
                    id: uuid::Uuid::nil(),
                    name: "示例网络区域".to_string(),
                    description: Some("这是一个示例网络区域".to_string()),
                    created_at: now,
                    updated_at: now,
                }],
                networks: vec![Network {
                    id: uuid::Uuid::nil(),
                    name: "示例网络".to_string(),
                    network_region_id: uuid::Uuid::nil(),
                    network_region: "示例网络区域".to_string(),
                    ipv4_cidr: Some("192.168.1.0/24".to_string()),
                    ipv6_cidr: None,
                    ipv4_gateway: Some("192.168.1.1".to_string()),
                    ipv6_gateway: None,
                    ipv4_dns: Some("8.8.8.8".to_string()),
                    ipv6_dns: None,
                    description: Some("这是一个示例网络".to_string()),
                    created_at: now,
                    updated_at: now,
                }],
                rooms: vec![Room {
                    id: uuid::Uuid::nil(),
                    name: "示例房间".to_string(),
                    room_type: "普通".to_string(),
                    description: Some("这是一个示例房间".to_string()),
                    created_at: now,
                    updated_at: now,
                }],
                workstations: vec![Workstation {
                    id: uuid::Uuid::nil(),
                    name: "示例工位".to_string(),
                    room_id: uuid::Uuid::nil(),
                    room_name: Some("示例房间".to_string()),
                    manager: Some("管理员".to_string()),
                    description: Some("这是一个示例工位".to_string()),
                    created_at: now,
                    updated_at: now,
                }],
                workstation_ports: vec![],
                ip_managers: vec![IpManagerWithNames {
                    id: uuid::Uuid::nil(),
                    workstation_id: None,
                    position_id: None,
                    network_id: uuid::Uuid::nil(),
                    workstation_name: Some("示例工位".to_string()),
                    cabinet_position_name: None,
                    switch_name: None,
                    switch_port_number: None,
                    network_name: "示例网络".to_string(),
                    network_region: "示例网络区域".to_string(),
                    ip_address: "192.168.1.100".to_string(),
                    ip_version: "IPv4".to_string(),
                    mac_address: Some("00:11:22:33:44:55".to_string()),
                    hostname: Some("example-host".to_string()),
                    status: "active".to_string(),
                    last_seen: now,
                    created_at: now,
                    updated_at: now,
                    device_name: None,
                    device_type: Some("workstation".to_string()),
                    switch_id: None,
                    switch_port_id: None,
                }],
            };

            // 返回JSON模板文件
            Ok(HttpResponse::Ok()
                .content_type("application/json")
                .append_header((
                    actix_web::http::header::CONTENT_DISPOSITION,
                    format!(
                        "attachment; filename=ipma_import_template_{}.json",
                        chrono::Utc::now().format("%Y%m%d_%H%M%S")
                    ),
                ))
                .json(template))
        }
        "csv" => {
            // 创建CSV模板内容
            let mut csv_data = Vec::new();

            // UTF-8 BOM标记
            let utf8_bom = &[0xEF, 0xBB, 0xBF];

            // 网络区域CSV模板
            let mut network_regions_csv = Vec::new();
            network_regions_csv.extend_from_slice(utf8_bom);
            network_regions_csv.extend_from_slice(
                "#table:network_regions\nID,名称,描述,创建时间,更新时间\n,,示例描述,,\n".as_bytes(),
            );
            csv_data.push(("network_regions_template.csv", network_regions_csv));

            // 网络CSV模板
            let mut networks_csv = Vec::new();
            networks_csv.extend_from_slice(utf8_bom);
            networks_csv.extend_from_slice("#table:networks\nID,名称,网络区域,IPv4 CIDR,IPv6 CIDR,IPv4网关,IPv6网关,IPv4 DNS,IPv6 DNS,描述,创建时间,更新时间\n,,示例网络区域,192.168.1.0/24,,,192.168.1.1,,,示例描述,,\n".as_bytes());
            csv_data.push(("networks_template.csv", networks_csv));

            // 房间CSV模板
            let mut rooms_csv = Vec::new();
            rooms_csv.extend_from_slice(utf8_bom);
            rooms_csv.extend_from_slice(
                "#table:rooms\nID,名称,描述,创建时间,更新时间\n,,示例描述,,\n".as_bytes(),
            );
            csv_data.push(("rooms_template.csv", rooms_csv));

            // 工位CSV模板
            let mut workstations_csv = Vec::new();
            workstations_csv.extend_from_slice(utf8_bom);
            workstations_csv.extend_from_slice("#table:workstations\nID,名称,房间,负责人,描述,创建时间,更新时间\n,,示例房间,管理员,示例描述,,\n".as_bytes());
            csv_data.push(("workstations_template.csv", workstations_csv));

            // IP管理CSV模板
            let mut ip_managers_csv = Vec::new();
            ip_managers_csv.extend_from_slice(utf8_bom);
            ip_managers_csv.extend_from_slice("#table:ip_managers\nID,工位,机位,网络,IP地址,IP版本,MAC地址,主机名,状态,最后在线,创建时间,更新时间\n,,示例工位,,192.168.1.100,IPv4,00:11:22:33:44:55,example-host,active,,\n".as_bytes());
            csv_data.push(("ip_managers_template.csv", ip_managers_csv));

            // 创建ZIP文件
            let mut buf = Cursor::new(Vec::new());
            let options = FileOptions::<'_, ()>::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644);

            {
                let mut zip = ZipWriter::new(&mut buf);

                // 将CSV模板文件添加到ZIP
                for (filename, data) in csv_data {
                    if let Err(err) = zip.start_file(filename, options) {
                        return Ok(HttpResponse::InternalServerError().json(
                            ApiResponse::<()>::error(format!("创建ZIP文件失败: {}", err)),
                        ));
                    }
                    if let Err(err) = zip.write_all(&data) {
                        return Ok(HttpResponse::InternalServerError().json(
                            ApiResponse::<()>::error(format!("写入ZIP文件失败: {}", err)),
                        ));
                    }
                }

                if let Err(err) = zip.finish() {
                    return Ok(
                        HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                            format!("完成ZIP文件失败: {}", err),
                        )),
                    );
                }
            }

            // 返回ZIP文件
            Ok(HttpResponse::Ok()
                .content_type("application/zip")
                .append_header((
                    actix_web::http::header::CONTENT_DISPOSITION,
                    format!(
                        "attachment; filename=ipma_import_template_{}.zip",
                        chrono::Utc::now().format("%Y%m%d_%H%M%S")
                    ),
                ))
                .body(buf.into_inner()))
        }
        _ => Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error("不支持的模板类型".to_string()))),
    }
}
