use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::{DateTime, Utc};
use sqlx::Row;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;
use validator::Validate;

use async_snmp::{Auth, Client, oid, v3::AuthProtocol};

use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, ArpEntry, SnmpTestRequest, Switch, SwitchCreate, SwitchPort, SwitchPortCreate,
    SwitchPortUpdate, SwitchPortWithSwitch, SwitchUpdate, SwitchWithParent,
};
use crate::utils::log_system_operation;

/// SNMP参数结构体
pub struct SnmpParams<'a> {
    pub ip: &'a str,
    pub port: i32,
    pub version: &'a str,
    pub community: Option<&'a str>,
    pub username: Option<&'a str>,
    pub auth_proto: Option<&'a str>,
    pub auth_pass: Option<&'a str>,
    pub priv_proto: Option<&'a str>,
    pub priv_pass: Option<&'a str>,
}

// ==================== 交换机管理 ====================

// 获取所有交换机
pub async fn get_switches(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    // 先测试简单查询，看看是否能连接数据库
    let test_query = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM switches")
        .fetch_one(pool.get_conn())
        .await;
    if let Err(e) = test_query {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "数据库连接测试失败: {}",
                e
            ))),
        );
    }

    let switches = sqlx::query_as::<_, SwitchWithParent>(
        r#"SELECT 
            s.id, s.name, s.network_region_id, s.network_id, CAST(s.ip_address AS TEXT) as ip_address, s.mac_address, s.model, s.vendor,
            CAST(s.management_ip AS TEXT) as management_ip, s.location, s.snmp_version, s.snmp_community,
            s.snmp_username, s.snmp_auth_protocol, s.snmp_auth_password,
            s.snmp_priv_protocol, s.snmp_priv_password, s.snmp_port,
            s.parent_switch_id, ps.name as parent_switch_name,
            s.parent_port_id, pp.port_number as parent_port_number,
            s.description, s.created_at, s.updated_at
        FROM switches s
        LEFT JOIN switches ps ON s.parent_switch_id = ps.id
        LEFT JOIN switch_ports pp ON s.parent_port_id = pp.id
        ORDER BY s.created_at DESC"#,
    )
    .fetch_all(pool.get_conn())
    .await;

    match switches {
        Ok(data) => Ok(HttpResponse::Ok().json(ApiResponse::success(data, "获取交换机列表成功"))),
        Err(e) => Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "获取交换机列表失败: {}",
                e
            ))),
        ),
    }
}

// 获取单个交换机
pub async fn get_switch(pool: web::Data<DbPool>, path: web::Path<Uuid>) -> Result<HttpResponse> {
    let id = path.into_inner();

    let switch = sqlx::query_as::<_, SwitchWithParent>(
        r#"SELECT 
            s.id, s.name, s.network_region_id, s.network_id, CAST(s.ip_address AS TEXT) as ip_address, s.mac_address, s.model, s.vendor,
            CAST(s.management_ip AS TEXT) as management_ip, s.location, s.snmp_version, s.snmp_community,
            s.snmp_username, s.snmp_auth_protocol, s.snmp_auth_password,
            s.snmp_priv_protocol, s.snmp_priv_password, s.snmp_port,
            s.parent_switch_id, ps.name as parent_switch_name,
            s.parent_port_id, pp.port_number as parent_port_number,
            s.description, s.created_at, s.updated_at
        FROM switches s
        LEFT JOIN switches ps ON s.parent_switch_id = ps.id
        LEFT JOIN switch_ports pp ON s.parent_port_id = pp.id
        WHERE s.id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool.get_conn())
    .await;

    match switch {
        Ok(Some(data)) => {
            // 获取交换机的IP列表，包含network_region_id
            let ips = sqlx::query(
                r#"SELECT 
                    m.id, m.switch_id, m.device_type, m.network_id, 
                    CAST(m.ip_address AS TEXT) as ip_address,
                    m.ip_version, m.mac_address, m.hostname,
                    m.status, m.last_seen, m.created_at, m.updated_at,
                    n.network_region_id
                FROM ip_managers m
                LEFT JOIN network_cidrs n ON m.network_id = n.id
                WHERE m.switch_id = $1
                ORDER BY m.ip_address"#,
            )
            .bind(id)
            .fetch_all(pool.get_conn())
            .await
            .unwrap_or_default();

            // 转换为JSON并添加network_region_id
            let ips_json: Vec<serde_json::Value> = ips.into_iter().map(|row| {
                serde_json::json!({
                    "id": row.get::<Uuid, _>(0),
                    "switch_id": row.get::<Option<Uuid>, _>(1),
                    "device_type": row.get::<Option<String>, _>(2),
                    "network_id": row.get::<Uuid, _>(3),
                    "ip_address": row.get::<String, _>(4),
                    "ip_version": row.get::<i16, _>(5),
                    "mac_address": row.get::<Option<String>, _>(6),
                    "hostname": row.get::<Option<String>, _>(7),
                    "status": row.get::<String, _>(8),
                    "last_seen": row.get::<DateTime<Utc>, _>(9),
                    "created_at": row.get::<DateTime<Utc>, _>(10),
                    "updated_at": row.get::<DateTime<Utc>, _>(11),
                    "network_region_id": row.get::<Uuid, _>(12)
                })
            }).collect();

            // 创建包含IP列表的响应对象
            let mut response_data = serde_json::to_value(data).unwrap();
            response_data["ips"] = serde_json::to_value(ips_json).unwrap();

            Ok(HttpResponse::Ok().json(ApiResponse::success(response_data, "获取交换机成功")))
        },
        Ok(None) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("获取交换机失败: {}", e)))),
    }
}

// 创建交换机
pub async fn create_switch(
    pool: web::Data<DbPool>,
    req: web::Json<SwitchCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证失败: {}", e)))
        );
    }

    // 检查是否至少有一个IP地址
    if req.ips.is_none() || req.ips.as_ref().unwrap().is_empty() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("至少需要添加一个IP地址")));
    }

    // 从ips数组中获取第一个IP的信息作为交换机的主IP
    let ips = req.ips.as_ref().unwrap();
    let first_ip = &ips[0];
    
    // 获取network_region_id
    let network_region_id = if let Some(nrid) = req.network_region_id {
        nrid
    } else if let Some(nrid) = first_ip.network_region_id {
        nrid
    } else {
        // 从network_id查询network_region_id
        match sqlx::query_scalar::<_, Uuid>(
            "SELECT network_region_id FROM network_cidrs WHERE id = $1"
        )
        .bind(first_ip.network_id)
        .fetch_optional(pool.get_conn())
        .await
        {
            Ok(Some(id)) => id,
            _ => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("无法获取网络区域ID"))),
        }
    };

    let id = Uuid::new_v4();
    let now = Utc::now();

    // 插入交换机记录
    let result = sqlx::query(
        r#"INSERT INTO switches (
            id, name, network_region_id, network_id, ip_address, model, vendor, management_ip,
            location, snmp_version, snmp_community, snmp_username,
            snmp_auth_protocol, snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port, parent_switch_id, parent_port_id,
            description, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, CAST($8 AS INET), $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22)"#
    )
    .bind(id)
    .bind(&req.name)
    .bind(network_region_id)
    .bind(first_ip.network_id)
    .bind(&first_ip.ip_address)
    .bind(&req.model)
    .bind(&req.vendor)
    .bind(&req.management_ip)
    .bind(&req.location)
    .bind(req.snmp_version.as_deref().unwrap_or("v2c"))
    .bind(&req.snmp_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(&req.snmp_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(&req.snmp_priv_password)
    .bind(req.snmp_port.unwrap_or(161))
    .bind(req.parent_switch_id)
    .bind(req.parent_port_id)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(_) => {
            // 处理所有IP地址
            for ip in ips {
                // 检查IP地址是否已存在
                let ip_exists = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM ip_managers WHERE ip_address = CAST($1 AS INET))",
                )
                .bind(&ip.ip_address)
                .fetch_one(pool.get_conn())
                .await
                .unwrap_or(false);

                if ip_exists {
                    return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("IP地址 {} 已存在", ip.ip_address))));
                }

                // 创建IP管理记录
                let ip_version: i16 = if ip.ip_address.contains(":") { 6 } else { 4 };
                let now = Utc::now();
                
                let ip_manager_id = Uuid::new_v4();
                let _ = sqlx::query(
                    "INSERT INTO ip_managers (id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
                     VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11, $12)"
                )
                .bind(ip_manager_id)
                .bind(id)
                .bind(ip.device_type.as_ref().unwrap_or(&"switch".to_string()))
                .bind(&ip.network_id)
                .bind(&ip.ip_address)
                .bind(ip_version)
                .bind(&ip.mac_address)
                .bind(&ip.hostname)
                .bind("active")
                .bind(now)
                .bind(now)
                .bind(now)
                .execute(pool.get_conn())
                .await;
            }

            // 查询新创建的交换机
            let switch = sqlx::query_as::<_, Switch>(
                r#"SELECT 
                    id, name, network_region_id, network_id,
                    CAST(ip_address AS TEXT) as ip_address, 
                    mac_address, model, vendor, 
                    CAST(management_ip AS TEXT) as management_ip, 
                    location, snmp_version, snmp_community, 
                    snmp_username, snmp_auth_protocol, snmp_auth_password, 
                    snmp_priv_protocol, snmp_priv_password, snmp_port, 
                    parent_switch_id, parent_port_id, description, created_at, updated_at 
                FROM switches WHERE id = $1"#,
            )
            .bind(id)
            .fetch_one(pool.get_conn())
            .await;

            match switch {
                Ok(data) => {
                    // 记录操作日志
                    let details = serde_json::json!({
                        "name": data.name,
                        "model": data.model,
                        "vendor": data.vendor,
                        "location": data.location,
                        "ip_count": req.ips.as_ref().unwrap_or(&vec![]).len()
                    });
                    let _ = log_system_operation(
                        pool.get_conn(),
                        &http_req,
                        config.get_ref(),
                        "create",
                        "switch",
                        &id,
                        &details,
                        true,
                    )
                    .await;
                    
                    Ok(HttpResponse::Ok().json(ApiResponse::success(data, "创建交换机成功")))
                }
                Err(e) => Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "创建交换机成功但查询失败: {}",
                        e
                    ))),
                ),
            }
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建交换机失败: {}", e)))),
    }
}

// 更新交换机
pub async fn update_switch(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    req: web::Json<SwitchUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证失败: {}", e)))
        );
    }

    // 检查交换机是否存在
    let exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM switches WHERE id = $1)")
            .bind(id)
            .fetch_one(pool.get_conn())
            .await
            .unwrap_or(false);

    if !exists {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
    }

    // 检查上级交换机循环引用
    if let Some(parent_switch_id) = req.parent_switch_id {
        if parent_switch_id == id {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error("不能将自己设置为上级交换机")));
        }
        
        // 检测是否存在循环引用
        if check_switch_cycle(pool.get_conn(), id, parent_switch_id).await? {
            return Ok(HttpResponse::BadRequest()
                .json(ApiResponse::<()>::error("检测到交换机层级循环引用，无法设置此上级交换机")));
        }
    }

    let now = Utc::now();

    // 构建动态更新SQL（不包含IP相关字段）
    let result = sqlx::query(
        r#"UPDATE switches SET
            name = COALESCE($1, name),
            model = COALESCE($2, model),
            vendor = COALESCE($3, vendor),
            management_ip = COALESCE(CAST($4 AS INET), management_ip),
            location = COALESCE($5, location),
            snmp_version = COALESCE($6, snmp_version),
            snmp_community = COALESCE($7, snmp_community),
            snmp_username = COALESCE($8, snmp_username),
            snmp_auth_protocol = COALESCE($9, snmp_auth_protocol),
            snmp_auth_password = COALESCE($10, snmp_auth_password),
            snmp_priv_protocol = COALESCE($11, snmp_priv_protocol),
            snmp_priv_password = COALESCE($12, snmp_priv_password),
            snmp_port = COALESCE($13, snmp_port),
            parent_switch_id = $14,
            parent_port_id = $15,
            description = COALESCE($16, description),
            updated_at = $17
        WHERE id = $18"#,
    )
    .bind(&req.name)
    .bind(&req.model)
    .bind(&req.vendor)
    .bind(&req.management_ip)
    .bind(&req.location)
    .bind(&req.snmp_version)
    .bind(&req.snmp_community)
    .bind(&req.snmp_username)
    .bind(&req.snmp_auth_protocol)
    .bind(&req.snmp_auth_password)
    .bind(&req.snmp_priv_protocol)
    .bind(&req.snmp_priv_password)
    .bind(req.snmp_port)
    .bind(req.parent_switch_id)
    .bind(req.parent_port_id)
    .bind(&req.description)
    .bind(now)
    .bind(id)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(_) => {
            // 处理IP地址更新
            if let Some(ips) = &req.ips {
                // 删除旧的IP管理记录
                let _ = sqlx::query("DELETE FROM ip_managers WHERE switch_id = $1")
                    .bind(id)
                    .execute(pool.get_conn())
                    .await;

                // 获取第一个IP的信息作为交换机的主IP
                if !ips.is_empty() {
                    let first_ip = &ips[0];
                    
                    // 获取network_region_id
                    let network_region_id = if let Some(nrid) = first_ip.network_region_id {
                        nrid
                    } else {
                        // 从network_id查询network_region_id
                        match sqlx::query_scalar::<_, Uuid>(
                            "SELECT network_region_id FROM network_cidrs WHERE id = $1"
                        )
                        .bind(first_ip.network_id)
                        .fetch_optional(pool.get_conn())
                        .await
                        {
                            Ok(Some(id)) => id,
                            _ => Uuid::nil(),
                        }
                    };
                    
                    // 更新交换机的主IP信息
                    let _ = sqlx::query(
                        "UPDATE switches SET network_region_id = $1, network_id = $2, ip_address = CAST($3 AS INET) WHERE id = $4"
                    )
                    .bind(network_region_id)
                    .bind(first_ip.network_id)
                    .bind(&first_ip.ip_address)
                    .bind(id)
                    .execute(pool.get_conn())
                    .await;
                }

                // 添加新的IP管理记录
                for ip in ips {
                    // 检查IP地址是否已被其他设备使用
                    let ip_exists = sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM ip_managers WHERE ip_address = CAST($1 AS INET) AND switch_id != $2)",
                    )
                    .bind(&ip.ip_address)
                    .bind(id)
                    .fetch_one(pool.get_conn())
                    .await
                    .unwrap_or(false);

                    if ip_exists {
                        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("IP地址 {} 已被其他设备使用", ip.ip_address))));
                    }

                    // 创建IP管理记录
                    let ip_version: i16 = if ip.ip_address.contains(":") { 6 } else { 4 };
                    let now = Utc::now();
                    
                    let ip_manager_id = Uuid::new_v4();
                    let _ = sqlx::query(
                        "INSERT INTO ip_managers (id, switch_id, device_type, network_id, ip_address, ip_version, mac_address, hostname, status, last_seen, created_at, updated_at) 
                         VALUES ($1, $2, $3, $4, CAST($5 AS INET), $6, $7, $8, $9, $10, $11, $12)"
                    )
                    .bind(ip_manager_id)
                    .bind(id)
                    .bind(ip.device_type.as_ref().unwrap_or(&"switch".to_string()))
                    .bind(&ip.network_id)
                    .bind(&ip.ip_address)
                    .bind(ip_version)
                    .bind(&ip.mac_address)
                    .bind(&ip.hostname)
                    .bind("active")
                    .bind(now)
                    .bind(now)
                    .bind(now)
                    .execute(pool.get_conn())
                    .await;
                }
            }

            // 查询更新后的交换机
            let switch = sqlx::query_as::<_, Switch>(
                r#"SELECT 
                    id, name, network_region_id, network_id,
                    CAST(ip_address AS TEXT) as ip_address, 
                    mac_address, model, vendor, 
                    CAST(management_ip AS TEXT) as management_ip, 
                    location, snmp_version, snmp_community, 
                    snmp_username, snmp_auth_protocol, snmp_auth_password, 
                    snmp_priv_protocol, snmp_priv_password, snmp_port, 
                    parent_switch_id, parent_port_id, description, created_at, updated_at 
                FROM switches WHERE id = $1"#,
            )
            .bind(id)
            .fetch_one(pool.get_conn())
            .await;

            match switch {
                Ok(data) => {
                    // 记录操作日志
                    let details = serde_json::json!({
                        "name": data.name,
                        "model": data.model,
                        "vendor": data.vendor,
                        "location": data.location,
                        "ip_count": req.ips.as_ref().unwrap_or(&vec![]).len()
                    });
                    let _ = log_system_operation(
                        pool.get_conn(),
                        &http_req,
                        config.get_ref(),
                        "update",
                        "switch",
                        &id,
                        &details,
                        true,
                    )
                    .await;
                    
                    Ok(HttpResponse::Ok().json(ApiResponse::success(data, "更新交换机成功")))
                }
                Err(e) => Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "更新交换机成功但查询失败: {}",
                        e
                    ))),
                ),
            }
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("更新交换机失败: {}", e)))),
    }
}

// 删除交换机
pub async fn delete_switch(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let id = path.into_inner();

    // 检查是否有子交换机
    let has_children = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM switches WHERE parent_switch_id = $1)",
    )
    .bind(id)
    .fetch_one(pool.get_conn())
    .await
    .unwrap_or(false);

    if has_children {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error("该交换机存在下级交换机，无法删除")));
    }

    let result = sqlx::query("DELETE FROM switches WHERE id = $1")
        .bind(id)
        .execute(pool.get_conn())
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            // 记录操作日志
            let details = serde_json::json!({
                "switch_id": id.to_string()
            });
            let _ = log_system_operation(
                pool.get_conn(),
                &http_req,
                config.get_ref(),
                "delete",
                "switch",
                &id,
                &details,
                true,
            )
            .await;

            Ok(HttpResponse::Ok().json(ApiResponse::success((), "删除交换机成功")))
        }
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("删除交换机失败: {}", e)))),
    }
}

// ==================== 交换机端口管理 ====================

// 获取交换机的所有端口
pub async fn get_switch_ports(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let ports = sqlx::query_as::<_, SwitchPort>(
        r#"SELECT * FROM switch_ports WHERE switch_id = $1 ORDER BY port_number"#,
    )
    .bind(switch_id)
    .fetch_all(pool.get_conn())
    .await;

    match ports {
        Ok(data) => Ok(HttpResponse::Ok().json(ApiResponse::success(data, "获取端口列表成功"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("获取端口列表失败: {}", e)))),
    }
}

// 获取所有交换机端口（带交换机信息）
pub async fn get_all_switch_ports(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let ports = sqlx::query_as::<_, SwitchPortWithSwitch>(
        r#"SELECT 
            sp.id, sp.switch_id, s.name as switch_name, CAST(s.ip_address AS TEXT) as switch_ip,
            sp.port_number, sp.port_name, sp.port_type, sp.vlan_id,
            sp.status, sp.speed, sp.description, sp.created_at, sp.updated_at
        FROM switch_ports sp
        JOIN switches s ON sp.switch_id = s.id
        ORDER BY s.name, sp.port_number"#,
    )
    .fetch_all(pool.get_conn())
    .await;

    match ports {
        Ok(data) => Ok(HttpResponse::Ok().json(ApiResponse::success(data, "获取所有端口列表成功"))),
        Err(e) => Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "获取所有端口列表失败: {}",
                e
            ))),
        ),
    }
}

// 创建交换机端口
pub async fn create_switch_port(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    req: web::Json<SwitchPortCreate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证失败: {}", e)))
        );
    }

    // 检查交换机是否存在
    let switch_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM switches WHERE id = $1)")
            .bind(switch_id)
            .fetch_one(pool.get_conn())
            .await
            .unwrap_or(false);

    if !switch_exists {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
    }

    // 检查端口号是否已存在
    let port_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM switch_ports WHERE switch_id = $1 AND port_number = $2)",
    )
    .bind(switch_id)
    .bind(&req.port_number)
    .fetch_one(pool.get_conn())
    .await
    .unwrap_or(false);

    if port_exists {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("该端口号已存在")));
    }

    let id = Uuid::new_v4();
    let now = Utc::now();

    let result = sqlx::query(
        r#"INSERT INTO switch_ports (
            id, switch_id, port_number, port_name, port_type, vlan_id,
            status, speed, description, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
    )
    .bind(id)
    .bind(switch_id)
    .bind(&req.port_number)
    .bind(&req.port_name)
    .bind(req.port_type.as_deref().unwrap_or("access"))
    .bind(req.vlan_id)
    .bind(req.status.as_deref().unwrap_or("up"))
    .bind(&req.speed)
    .bind(&req.description)
    .bind(now)
    .bind(now)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(_) => {
            let port = sqlx::query_as::<_, SwitchPort>("SELECT * FROM switch_ports WHERE id = $1")
                .bind(id)
                .fetch_one(pool.get_conn())
                .await;

            match port {
                Ok(data) => {
                    // 记录操作日志
                    let details = serde_json::json!({
                        "switch_id": switch_id,
                        "port_number": data.port_number,
                        "port_name": data.port_name,
                        "port_type": data.port_type,
                        "vlan_id": data.vlan_id
                    });
                    let _ = log_system_operation(
                        pool.get_conn(),
                        &http_req,
                        config.get_ref(),
                        "create",
                        "switch_port",
                        &id,
                        &details,
                        true,
                    )
                    .await;

                    Ok(HttpResponse::Ok().json(ApiResponse::success(data, "创建端口成功")))
                }
                Err(e) => Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "创建端口成功但查询失败: {}",
                        e
                    ))),
                ),
            }
        }
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("创建端口失败: {}", e)))),
    }
}

// 获取单个端口
pub async fn get_switch_port(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let port_id = path.into_inner();

    let port = sqlx::query_as::<_, SwitchPortWithSwitch>(
        r#"SELECT 
            sp.id, sp.switch_id, s.name as switch_name, CAST(s.ip_address AS inet) as switch_ip,
            sp.port_number, sp.port_name, sp.port_type, sp.vlan_id,
            sp.status, sp.speed, sp.description, sp.created_at, sp.updated_at
        FROM switch_ports sp
        JOIN switches s ON sp.switch_id = s.id
        WHERE sp.id = $1"#,
    )
    .bind(port_id)
    .fetch_optional(pool.get_conn())
    .await;

    match port {
        Ok(Some(data)) => Ok(HttpResponse::Ok().json(ApiResponse::success(data, "获取端口成功"))),
        Ok(None) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("端口不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("获取端口失败: {}", e)))),
    }
}

// 更新交换机端口
pub async fn update_switch_port(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    req: web::Json<SwitchPortUpdate>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let port_id = path.into_inner();

    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证失败: {}", e)))
        );
    }

    let now = Utc::now();

    let result = sqlx::query(
        r#"UPDATE switch_ports SET
            port_number = COALESCE($1, port_number),
            port_name = COALESCE($2, port_name),
            port_type = COALESCE($3, port_type),
            vlan_id = COALESCE($4, vlan_id),
            status = COALESCE($5, status),
            speed = COALESCE($6, speed),
            description = COALESCE($7, description),
            updated_at = $8
        WHERE id = $9"#,
    )
    .bind(&req.port_number)
    .bind(&req.port_name)
    .bind(&req.port_type)
    .bind(req.vlan_id)
    .bind(&req.status)
    .bind(&req.speed)
    .bind(&req.description)
    .bind(now)
    .bind(port_id)
    .execute(pool.get_conn())
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            let port = sqlx::query_as::<_, SwitchPort>("SELECT * FROM switch_ports WHERE id = $1")
                .bind(port_id)
                .fetch_one(pool.get_conn())
                .await;

            match port {
                Ok(data) => {
                    // 记录操作日志
                    let details = serde_json::json!({
                        "switch_id": data.switch_id,
                        "port_number": data.port_number,
                        "port_name": data.port_name,
                        "port_type": data.port_type,
                        "vlan_id": data.vlan_id
                    });
                    let _ = log_system_operation(
                        pool.get_conn(),
                        &http_req,
                        config.get_ref(),
                        "update",
                        "switch_port",
                        &port_id,
                        &details,
                        true,
                    )
                    .await;

                    Ok(HttpResponse::Ok().json(ApiResponse::success(data, "更新端口成功")))
                }
                Err(e) => Ok(
                    HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                        "更新端口成功但查询失败: {}",
                        e
                    ))),
                ),
            }
        }
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("端口不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("更新端口失败: {}", e)))),
    }
}

// 删除交换机端口
pub async fn delete_switch_port(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let port_id = path.into_inner();

    // 检查是否有交换机连接到此端口
    let has_child_switch = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM switches WHERE parent_port_id = $1)",
    )
    .bind(port_id)
    .fetch_one(pool.get_conn())
    .await
    .unwrap_or(false);

    if has_child_switch {
        return Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error("该端口有下级交换机连接，无法删除")));
    }

    // 检查是否有工位关联到此端口
    let has_workstation = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workstation_ports WHERE switch_port_id = $1)",
    )
    .bind(port_id)
    .fetch_one(pool.get_conn())
    .await
    .unwrap_or(false);

    if has_workstation {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("该端口有工位关联，无法删除"))
        );
    }

    // 检查是否有机位关联到此端口
    let has_cabinet_position = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM cabinet_position_ports WHERE switch_port_id = $1)",
    )
    .bind(port_id)
    .fetch_one(pool.get_conn())
    .await
    .unwrap_or(false);

    if has_cabinet_position {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("该端口有机位关联，无法删除"))
        );
    }

    let result = sqlx::query("DELETE FROM switch_ports WHERE id = $1")
        .bind(port_id)
        .execute(pool.get_conn())
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            // 记录操作日志
            let details = serde_json::json!({
                "port_id": port_id
            });
            let _ = log_system_operation(
                pool.get_conn(),
                &http_req,
                config.get_ref(),
                "delete",
                "switch_port",
                &port_id,
                &details,
                true,
            )
            .await;

            Ok(HttpResponse::Ok().json(ApiResponse::success((), "删除端口成功")))
        }
        Ok(_) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("端口不存在"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("删除端口失败: {}", e)))),
    }
}

// ==================== SNMP功能 ====================

// 测试SNMP连接
pub async fn test_snmp_connection(
    pool: web::Data<DbPool>,
    req: web::Json<SnmpTestRequest>,
) -> Result<HttpResponse> {
    // 获取SNMP配置
    let (ip, version, community, username, auth_proto, auth_pass, priv_proto, priv_pass, port) =
        if let Some(switch_id) = req.switch_id {
            // 从数据库获取交换机配置
            let switch = sqlx::query_as::<_, Switch>(
                r#"SELECT 
                    id, name, network_region_id, network_id,
                    CAST(ip_address AS TEXT) as ip_address, 
                    mac_address, model, vendor, 
                    CAST(management_ip AS TEXT) as management_ip, 
                    location, snmp_version, snmp_community, 
                    snmp_username, snmp_auth_protocol, snmp_auth_password, 
                    snmp_priv_protocol, snmp_priv_password, snmp_port, 
                    parent_switch_id, parent_port_id, description, created_at, updated_at 
                FROM switches WHERE id = $1"#,
            )
            .bind(switch_id)
            .fetch_optional(pool.get_conn())
            .await;

            match switch {
                Ok(Some(s)) => (
                    s.ip_address,
                    s.snmp_version,
                    s.snmp_community,
                    s.snmp_username,
                    s.snmp_auth_protocol,
                    s.snmp_auth_password,
                    s.snmp_priv_protocol,
                    s.snmp_priv_password,
                    s.snmp_port,
                ),
                Ok(None) => {
                    return Ok(
                        HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在"))
                    );
                }
                Err(e) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("查询交换机失败: {}", e))));
                }
            }
        } else {
            // 使用请求中的配置
            (
                req.ip_address.clone().unwrap_or_default(),
                req.snmp_version
                    .clone()
                    .unwrap_or_else(|| "v2c".to_string()),
                req.snmp_community.clone(),
                req.snmp_username.clone(),
                req.snmp_auth_protocol.clone(),
                req.snmp_auth_password.clone(),
                req.snmp_priv_protocol.clone(),
                req.snmp_priv_password.clone(),
                req.snmp_port.unwrap_or(161),
            )
        };

    if ip.is_empty() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("IP地址不能为空")));
    }

    // 执行SNMP测试
    let snmp_params = SnmpParams {
        ip: &ip,
        port,
        version: &version,
        community: community.as_deref(),
        username: username.as_deref(),
        auth_proto: auth_proto.as_deref(),
        auth_pass: auth_pass.as_deref(),
        priv_proto: priv_proto.as_deref(),
        priv_pass: priv_pass.as_deref(),
    };

    match test_snmp(&snmp_params).await {
        Ok(sys_descr) => Ok(HttpResponse::Ok().json(ApiResponse::success(
            serde_json::json!({ "sysDescr": sys_descr }),
            "SNMP连接测试成功",
        ))),
        Err(e) => Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(format!("SNMP连接测试失败: {}", e)))),
    }
}

// 获取交换机ARP表
pub async fn get_switch_arp_table(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let switch = sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            CAST(ip_address AS TEXT) as ip_address, 
            mac_address, model, vendor, 
            CAST(management_ip AS TEXT) as management_ip, 
            location, snmp_version, snmp_community, 
            snmp_username, snmp_auth_protocol, snmp_auth_password, 
            snmp_priv_protocol, snmp_priv_password, snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
        FROM switches WHERE id = $1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool.get_conn())
    .await;

    let switch = match switch {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
        }
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("查询交换机失败: {}", e))));
        }
    };

    // 获取ARP表
    let snmp_params = SnmpParams {
        ip: &switch.ip_address,
        port: switch.snmp_port,
        version: &switch.snmp_version,
        community: switch.snmp_community.as_deref(),
        username: switch.snmp_username.as_deref(),
        auth_proto: switch.snmp_auth_protocol.as_deref(),
        auth_pass: switch.snmp_auth_password.as_deref(),
        priv_proto: switch.snmp_priv_protocol.as_deref(),
        priv_pass: switch.snmp_priv_password.as_deref(),
    };

    match get_arp_table_via_snmp(&snmp_params).await {
        Ok(entries) => Ok(HttpResponse::Ok().json(ApiResponse::success(entries, "获取ARP表成功"))),
        Err(e) => Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(format!("获取ARP表失败: {}", e)))),
    }
}

// 从SNMP获取设备基本信息（型号和厂商）
async fn get_switch_info_via_snmp(
    snmp_params: &SnmpParams<'_>,
) -> std::result::Result<(String, String), String> {
    let addr = format!("{}:{}", snmp_params.ip, snmp_params.port);
    let timeout = Duration::from_secs(5);

    // 根据SNMP版本创建不同的认证方式
    let auth = match snmp_params.version {
        "v1" | "v2c" => {
            let community = snmp_params.community.unwrap_or("public");
            Auth::v2c(community)
        }
        "v3" => {
            let username = snmp_params.username.ok_or("SNMPv3需要用户名".to_string())?;
            let mut auth = Auth::usm(username);

            // 添加认证信息
            if let (Some(proto), Some(pass)) = (snmp_params.auth_proto, snmp_params.auth_pass) {
                let auth_protocol = match proto {
                    "MD5" => AuthProtocol::Md5,
                    "SHA" => AuthProtocol::Sha1,
                    "SHA-224" => AuthProtocol::Sha224,
                    "SHA-256" => AuthProtocol::Sha256,
                    "SHA-384" => AuthProtocol::Sha384,
                    "SHA-512" => AuthProtocol::Sha512,
                    _ => return Err(format!("不支持的认证协议: {}", proto)),
                };
                auth = auth.auth(auth_protocol, pass);

                // 添加隐私信息
                if let (Some(proto), Some(pass)) = (snmp_params.priv_proto, snmp_params.priv_pass) {
                    let priv_protocol = match proto {
                        "DES" => async_snmp::v3::PrivProtocol::Des,
                        "3DES" => async_snmp::v3::PrivProtocol::Des3,
                        "AES" => async_snmp::v3::PrivProtocol::Aes128,
                        "AES-192" => async_snmp::v3::PrivProtocol::Aes192,
                        "AES-256" => async_snmp::v3::PrivProtocol::Aes256,
                        _ => return Err(format!("不支持的隐私协议: {}", proto)),
                    };
                    auth = auth.privacy(priv_protocol, pass);
                }
            }

            auth.into()
        }
        _ => {
            return Err(format!("不支持的SNMP版本: {}", snmp_params.version));
        }
    };

    // 创建客户端
    let client = Client::builder(&addr, auth)
        .timeout(timeout)
        .connect()
        .await
        .map_err(|e| format!("创建SNMP会话失败: {:?}", e))?;

    // 获取sysDescr.0 (1.3.6.1.2.1.1.1.0) - 设备描述
    let result = client
        .get(&oid!(1, 3, 6, 1, 2, 1, 1, 1, 0))
        .await
        .map_err(|e| format!("SNMP GET请求失败: {:?}", e))?;

    // 处理响应
    match result.value {
        async_snmp::Value::OctetString(bytes) => {
            let sys_descr = String::from_utf8(bytes.to_vec())
                .map_err(|e| format!("响应格式不正确: {:?}", e))?;

            // 从sysDescr中提取厂商和型号信息
            let (vendor, model) = parse_sys_descr(&sys_descr);

            Ok((vendor, model))
        }
        _ => Err("响应格式不正确".to_string()),
    }
}

// 解析sysDescr获取厂商和型号
fn parse_sys_descr(sys_descr: &str) -> (String, String) {
    // 简单的厂商和型号解析逻辑
    // 实际应用中可能需要更复杂的解析或使用OID映射表
    let lower_descr = sys_descr.to_lowercase();

    // 常见厂商关键字
    let vendors = [
        ("cisco", "Cisco"),
        ("huawei", "Huawei"),
        ("h3c", "H3C"),
        ("juniper", "Juniper"),
        ("dell", "Dell"),
        ("hp", "HP"),
        ("aruba", "Aruba"),
        ("netgear", "Netgear"),
        ("tp-link", "TP-Link"),
        ("linksys", "Linksys"),
    ];

    let mut vendor = "Unknown".to_string();
    for (keyword, full_name) in vendors {
        if lower_descr.contains(keyword) {
            vendor = full_name.to_string();
            break;
        }
    }

    // 提取型号（简单实现）
    let model = sys_descr
        .split_whitespace()
        .filter(|word| word.chars().any(|c| c.is_ascii_digit()))
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");

    (
        vendor,
        if model.is_empty() {
            sys_descr.to_string()
        } else {
            model
        },
    )
}

// SNMP测试实现
async fn test_snmp(snmp_params: &SnmpParams<'_>) -> std::result::Result<String, String> {
    let addr = format!("{}:{}", snmp_params.ip, snmp_params.port);
    let timeout = Duration::from_secs(5);

    // 根据SNMP版本创建不同的认证方式
    let auth = match snmp_params.version {
        "v1" | "v2c" => {
            let community = snmp_params.community.unwrap_or("public");
            Auth::v2c(community)
        }
        "v3" => {
            let username = snmp_params.username.ok_or("SNMPv3需要用户名".to_string())?;
            let mut auth = Auth::usm(username);

            // 添加认证信息
            if let (Some(proto), Some(pass)) = (snmp_params.auth_proto, snmp_params.auth_pass) {
                let auth_protocol = match proto {
                    "MD5" => AuthProtocol::Md5,
                    "SHA" => AuthProtocol::Sha1,
                    "SHA-224" => AuthProtocol::Sha224,
                    "SHA-256" => AuthProtocol::Sha256,
                    "SHA-384" => AuthProtocol::Sha384,
                    "SHA-512" => AuthProtocol::Sha512,
                    _ => return Err(format!("不支持的认证协议: {}", proto)),
                };
                auth = auth.auth(auth_protocol, pass);

                // 添加隐私信息
                if let (Some(proto), Some(pass)) = (snmp_params.priv_proto, snmp_params.priv_pass) {
                    let priv_protocol = match proto {
                        "DES" => async_snmp::v3::PrivProtocol::Des,
                        "3DES" => async_snmp::v3::PrivProtocol::Des3,
                        "AES" => async_snmp::v3::PrivProtocol::Aes128,
                        "AES-192" => async_snmp::v3::PrivProtocol::Aes192,
                        "AES-256" => async_snmp::v3::PrivProtocol::Aes256,
                        _ => return Err(format!("不支持的隐私协议: {}", proto)),
                    };
                    auth = auth.privacy(priv_protocol, pass);
                }
            }

            auth.into()
        }
        _ => {
            return Err(format!("不支持的SNMP版本: {}", snmp_params.version));
        }
    };

    // 创建客户端
    let client = Client::builder(&addr, auth)
        .timeout(timeout)
        .connect()
        .await
        .map_err(|e| format!("创建SNMP会话失败: {:?}", e))?;

    // 获取sysDescr.0 (1.3.6.1.2.1.1.1.0)
    let result = client
        .get(&oid!(1, 3, 6, 1, 2, 1, 1, 1, 0))
        .await
        .map_err(|e| format!("SNMP GET请求失败: {:?}", e))?;

    // 处理响应
    match result.value {
        async_snmp::Value::OctetString(bytes) => {
            String::from_utf8(bytes.to_vec()).map_err(|e| format!("响应格式不正确: {:?}", e))
        }
        _ => Err("响应格式不正确".to_string()),
    }
}

// 通过SNMP获取ARP表
async fn get_arp_table_via_snmp(
    snmp_params: &SnmpParams<'_>,
) -> std::result::Result<Vec<ArpEntry>, String> {
    let addr = format!("{}:{}", snmp_params.ip, snmp_params.port);
    let timeout = Duration::from_secs(10);

    // 根据SNMP版本创建不同的认证方式
    let auth = match snmp_params.version {
        "v1" | "v2c" => {
            let community = snmp_params.community.unwrap_or("public");
            Auth::v2c(community)
        }
        "v3" => {
            let username = snmp_params.username.ok_or("SNMPv3需要用户名".to_string())?;
            let mut auth = Auth::usm(username);

            // 添加认证信息
            if let (Some(proto), Some(pass)) = (snmp_params.auth_proto, snmp_params.auth_pass) {
                let auth_protocol = match proto {
                    "MD5" => AuthProtocol::Md5,
                    "SHA" => AuthProtocol::Sha1,
                    "SHA-224" => AuthProtocol::Sha224,
                    "SHA-256" => AuthProtocol::Sha256,
                    "SHA-384" => AuthProtocol::Sha384,
                    "SHA-512" => AuthProtocol::Sha512,
                    _ => return Err(format!("不支持的认证协议: {}", proto)),
                };
                auth = auth.auth(auth_protocol, pass);

                // 添加隐私信息
                if let (Some(proto), Some(pass)) = (snmp_params.priv_proto, snmp_params.priv_pass) {
                    let priv_protocol = match proto {
                        "DES" => async_snmp::v3::PrivProtocol::Des,
                        "3DES" => async_snmp::v3::PrivProtocol::Des3,
                        "AES" => async_snmp::v3::PrivProtocol::Aes128,
                        "AES-192" => async_snmp::v3::PrivProtocol::Aes192,
                        "AES-256" => async_snmp::v3::PrivProtocol::Aes256,
                        _ => return Err(format!("不支持的隐私协议: {}", proto)),
                    };
                    auth = auth.privacy(priv_protocol, pass);
                }
            }

            auth.into()
        }
        _ => {
            return Err(format!("不支持的SNMP版本: {}", snmp_params.version));
        }
    };

    // 创建客户端
    let client = Client::builder(&addr, auth)
        .timeout(timeout)
        .connect()
        .await
        .map_err(|e| format!("创建SNMP会话失败: {:?}", e))?;

    let mut entries = Vec::new();

    // ipNetToMediaPhysAddress OID: 1.3.6.1.2.1.4.22.1.2
    let arp_oid = oid!(1, 3, 6, 1, 2, 1, 4, 22, 1, 2);

    // 使用walk遍历ARP表
    let mut walk = client
        .walk(arp_oid)
        .map_err(|e| format!("创建SNMP walk失败: {:?}", e))?;

    while let Some(result) = walk.next().await {
        let vb = result.map_err(|e| format!("SNMP walk失败: {:?}", e))?;

        // 提取IP地址 (OID最后4个数字)
        let oid_parts = vb.oid.arcs();
        if oid_parts.len() >= 14 {
            let ip_addr = format!(
                "{}.{}.{}.{}",
                oid_parts[oid_parts.len() - 4],
                oid_parts[oid_parts.len() - 3],
                oid_parts[oid_parts.len() - 2],
                oid_parts[oid_parts.len() - 1]
            );

            // 提取MAC地址
            if let async_snmp::Value::OctetString(bytes) = vb.value {
                let bytes_vec = bytes.to_vec();
                if bytes_vec.len() >= 6 {
                    let mac_addr = format!(
                        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        bytes_vec[0],
                        bytes_vec[1],
                        bytes_vec[2],
                        bytes_vec[3],
                        bytes_vec[4],
                        bytes_vec[5]
                    );

                    entries.push(ArpEntry {
                        ip_address: ip_addr,
                        mac_address: mac_addr,
                        interface: None,
                    });
                }
            }
        }
    }

    Ok(entries)
}

// 批量通过SNMP获取MAC地址
pub async fn batch_get_mac_via_snmp(
    pool: &sqlx::PgPool,
    ips: &[String],
) -> HashMap<String, Option<String>> {
    let mut results: HashMap<String, Option<String>> = HashMap::new();

    // 获取所有交换机
    let switches = sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            CAST(ip_address AS TEXT) as ip_address, 
            mac_address, model, vendor, 
            CAST(management_ip AS TEXT) as management_ip, 
            location, snmp_version, snmp_community, 
            snmp_username, snmp_auth_protocol, snmp_auth_password, 
            snmp_priv_protocol, snmp_priv_password, snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
        FROM switches WHERE snmp_community IS NOT NULL OR snmp_username IS NOT NULL"#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if switches.is_empty() {
        // 没有配置SNMP的交换机，返回空结果
        for ip in ips {
            results.insert(ip.clone(), None);
        }
        return results;
    }

    // 从所有交换机获取ARP表
    let mut all_arp_entries: HashMap<String, String> = HashMap::new();

    for switch in &switches {
        let snmp_params = SnmpParams {
            ip: &switch.ip_address,
            port: switch.snmp_port,
            version: &switch.snmp_version,
            community: switch.snmp_community.as_deref(),
            username: switch.snmp_username.as_deref(),
            auth_proto: switch.snmp_auth_protocol.as_deref(),
            auth_pass: switch.snmp_auth_password.as_deref(),
            priv_proto: switch.snmp_priv_protocol.as_deref(),
            priv_pass: switch.snmp_priv_password.as_deref(),
        };

        if let Ok(entries) = get_arp_table_via_snmp(&snmp_params).await {
            for entry in entries {
                all_arp_entries.insert(entry.ip_address, entry.mac_address);
            }
        }
    }

    // 匹配请求的IP
    for ip in ips {
        let mac = all_arp_entries.get(ip).cloned();
        results.insert(ip.clone(), mac);
    }

    results
}

// 从指定交换机通过SNMP获取MAC地址
pub async fn get_mac_from_switch(
    pool: &sqlx::PgPool,
    switch_id: &uuid::Uuid,
    ips: &[String],
) -> std::result::Result<HashMap<String, Option<String>>, String> {
    let mut results: HashMap<String, Option<String>> = HashMap::new();

    // 获取指定交换机
    let switch = sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            CAST(ip_address AS TEXT) as ip_address, 
            mac_address, model, vendor, 
            CAST(management_ip AS TEXT) as management_ip, 
            location, snmp_version, snmp_community, 
            snmp_username, snmp_auth_protocol, snmp_auth_password, 
            snmp_priv_protocol, snmp_priv_password, snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
        FROM switches WHERE id = $1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("查询交换机失败: {}", e))?;

    let switch = match switch {
        Some(s) => s,
        None => return Err("交换机不存在".to_string()),
    };

    // 检查SNMP配置
    if switch.snmp_community.is_none() && switch.snmp_username.is_none() {
        return Err("该交换机未配置SNMP".to_string());
    }

    // 从交换机获取ARP表
    let snmp_params = SnmpParams {
        ip: &switch.ip_address,
        port: switch.snmp_port,
        version: &switch.snmp_version,
        community: switch.snmp_community.as_deref(),
        username: switch.snmp_username.as_deref(),
        auth_proto: switch.snmp_auth_protocol.as_deref(),
        auth_pass: switch.snmp_auth_password.as_deref(),
        priv_proto: switch.snmp_priv_protocol.as_deref(),
        priv_pass: switch.snmp_priv_password.as_deref(),
    };

    let entries = get_arp_table_via_snmp(&snmp_params).await?;

    // 构建IP->MAC映射
    let arp_map: HashMap<String, String> = entries
        .into_iter()
        .map(|e| (e.ip_address, e.mac_address))
        .collect();

    // 匹配请求的IP
    for ip in ips {
        let mac = arp_map.get(ip).cloned();
        results.insert(ip.clone(), mac);
    }

    Ok(results)
}

// 通过SNMP获取交换机端口信息
async fn get_switch_ports_via_snmp(
    snmp_params: &SnmpParams<'_>,
) -> std::result::Result<Vec<SwitchPortCreate>, String> {
    let addr = format!("{}:{}", snmp_params.ip, snmp_params.port);
    let timeout = Duration::from_secs(10);

    // 根据SNMP版本创建不同的认证方式
    let auth = match snmp_params.version {
        "v1" | "v2c" => {
            let community = snmp_params.community.unwrap_or("public");
            Auth::v2c(community)
        }
        "v3" => {
            let username = snmp_params.username.ok_or("SNMPv3需要用户名".to_string())?;
            let mut auth = Auth::usm(username);

            // 添加认证信息
            if let (Some(proto), Some(pass)) = (snmp_params.auth_proto, snmp_params.auth_pass) {
                let auth_protocol = match proto {
                    "MD5" => AuthProtocol::Md5,
                    "SHA" => AuthProtocol::Sha1,
                    "SHA-224" => AuthProtocol::Sha224,
                    "SHA-256" => AuthProtocol::Sha256,
                    "SHA-384" => AuthProtocol::Sha384,
                    "SHA-512" => AuthProtocol::Sha512,
                    _ => return Err(format!("不支持的认证协议: {}", proto)),
                };
                auth = auth.auth(auth_protocol, pass);

                // 添加隐私信息
                if let (Some(proto), Some(pass)) = (snmp_params.priv_proto, snmp_params.priv_pass) {
                    let priv_protocol = match proto {
                        "DES" => async_snmp::v3::PrivProtocol::Des,
                        "3DES" => async_snmp::v3::PrivProtocol::Des3,
                        "AES" => async_snmp::v3::PrivProtocol::Aes128,
                        "AES-192" => async_snmp::v3::PrivProtocol::Aes192,
                        "AES-256" => async_snmp::v3::PrivProtocol::Aes256,
                        _ => return Err(format!("不支持的隐私协议: {}", proto)),
                    };
                    auth = auth.privacy(priv_protocol, pass);
                }
            }

            auth.into()
        }
        _ => {
            return Err(format!("不支持的SNMP版本: {}", snmp_params.version));
        }
    };

    // 创建客户端
    let client = Client::builder(&addr, auth)
        .timeout(timeout)
        .connect()
        .await
        .map_err(|e| format!("创建SNMP会话失败: {:?}", e))?;

    let mut ports = Vec::new();

    // ifDescr OID: 1.3.6.1.2.1.2.2.1.2
    let if_descr_oid = oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2);

    // 使用walk遍历所有接口
    let mut walk = client
        .walk(if_descr_oid)
        .map_err(|e| format!("创建SNMP walk失败: {:?}", e))?;

    while let Some(result) = walk.next().await {
        let vb = result.map_err(|e| format!("SNMP walk失败: {:?}", e))?;

        // 提取ifIndex
        let oid_parts = vb.oid.arcs();
        let if_index = oid_parts.last().unwrap_or(&0).to_string();

        // 获取端口描述
        let port_number = match vb.value {
            async_snmp::Value::OctetString(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            _ => if_index.clone(),
        };

        // 创建端口对象
        let port_create = SwitchPortCreate {
            port_number: port_number.clone(),
            port_name: None,
            port_type: None,
            vlan_id: None,
            status: None,
            speed: None,
            description: Some(port_number),
        };

        // 添加到端口列表
        ports.push(port_create);
    }

    Ok(ports)
}

// 通过SNMP获取交换机基本信息API
pub async fn get_switch_info_snmp(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let switch = sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            CAST(ip_address AS TEXT) as ip_address, 
            mac_address, model, vendor, 
            CAST(management_ip AS TEXT) as management_ip, 
            location, snmp_version, snmp_community, 
            snmp_username, snmp_auth_protocol, snmp_auth_password, 
            snmp_priv_protocol, snmp_priv_password, snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
        FROM switches WHERE id = $1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool.get_conn())
    .await;

    let switch = match switch {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
        }
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("查询交换机失败: {}", e))));
        }
    };

    // 获取交换机信息
    let snmp_params = SnmpParams {
        ip: &switch.ip_address,
        port: switch.snmp_port,
        version: &switch.snmp_version,
        community: switch.snmp_community.as_deref(),
        username: switch.snmp_username.as_deref(),
        auth_proto: switch.snmp_auth_protocol.as_deref(),
        auth_pass: switch.snmp_auth_password.as_deref(),
        priv_proto: switch.snmp_priv_protocol.as_deref(),
        priv_pass: switch.snmp_priv_password.as_deref(),
    };

    match get_switch_info_via_snmp(&snmp_params).await {
        Ok((vendor, model)) => Ok(HttpResponse::Ok().json(ApiResponse::success(
            serde_json::json!({ "vendor": vendor, "model": model }),
            "获取交换机信息成功",
        ))),
        Err(e) => Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "获取交换机信息失败: {}",
                e
            ))),
        ),
    }
}

// 通过SNMP获取交换机端口信息API
pub async fn get_switch_ports_snmp(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let switch = sqlx::query_as::<_, Switch>(
        r#"SELECT 
            id, name, network_region_id, network_id,
            CAST(ip_address AS TEXT) as ip_address, 
            mac_address, model, vendor, 
            CAST(management_ip AS TEXT) as management_ip, 
            location, snmp_version, snmp_community, 
            snmp_username, snmp_auth_protocol, snmp_auth_password, 
            snmp_priv_protocol, snmp_priv_password, snmp_port, 
            parent_switch_id, parent_port_id, description, created_at, updated_at 
        FROM switches WHERE id = $1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool.get_conn())
    .await;

    let switch = match switch {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
        }
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("查询交换机失败: {}", e))));
        }
    };

    // 获取端口信息
    let snmp_params = SnmpParams {
        ip: &switch.ip_address,
        port: switch.snmp_port,
        version: &switch.snmp_version,
        community: switch.snmp_community.as_deref(),
        username: switch.snmp_username.as_deref(),
        auth_proto: switch.snmp_auth_protocol.as_deref(),
        auth_pass: switch.snmp_auth_password.as_deref(),
        priv_proto: switch.snmp_priv_protocol.as_deref(),
        priv_pass: switch.snmp_priv_password.as_deref(),
    };

    match get_switch_ports_via_snmp(&snmp_params).await {
        Ok(ports) => {
            Ok(HttpResponse::Ok().json(ApiResponse::success(ports, "获取交换机端口信息成功")))
        }
        Err(e) => Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "获取交换机端口信息失败: {}",
                e
            ))),
        ),
    }
}

// 检测交换机层级是否存在循环引用
async fn check_switch_cycle(
    pool: &sqlx::PgPool,
    switch_id: Uuid,
    parent_id: Uuid,
) -> Result<bool> {
    let mut current = parent_id;
    let mut visited = std::collections::HashSet::new();
    
    while !visited.contains(&current) {
        if current == switch_id {
            return Ok(true);
        }
        visited.insert(current);
        
        let next_parent: Option<Uuid> = match sqlx::query_scalar(
            "SELECT parent_switch_id FROM switches WHERE id = $1"
        )
        .bind(current)
        .fetch_optional(pool)
        .await
        {
            Ok(Some(id)) => id,
            Ok(None) => break,
            Err(_) => break,
        };
        
        match next_parent {
            Some(id) => current = id,
            None => break,
        }
    }
    
    Ok(false)
}
