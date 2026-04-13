use actix_web::{HttpRequest, HttpResponse, Result, web};
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

use super::snmp::{SwitchForSnmp, get_switch_ports_via_snmp};
use crate::config::Config;
use crate::db::DbPool;
use crate::models::{
    ApiResponse, SwitchPort, SwitchPortCreate, SwitchPortUpdate, SwitchPortWithSwitch,
};
use crate::utils::log_system_operation;

pub async fn get_switch_ports(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let offset = (page - 1) * page_size;

    let total: i64 =
        match sqlx::query_scalar("SELECT COUNT(*) FROM switch_ports WHERE switch_id = $1")
            .bind(switch_id)
            .fetch_one(pool.get_conn())
            .await
        {
            Ok(t) => t,
            Err(e) => {
                return Ok(HttpResponse::InternalServerError()
                    .json(ApiResponse::<()>::error(format!("获取端口数量失败: {}", e))));
            }
        };

    let ports = sqlx::query_as::<_, SwitchPort>(
        r#"SELECT * FROM switch_ports WHERE switch_id = $1 ORDER BY port_number LIMIT $2 OFFSET $3"#,
    )
    .bind(switch_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(pool.get_conn())
    .await;

    match ports {
        Ok(data) => Ok(HttpResponse::Ok().json(ApiResponse::success(
            serde_json::json!({
                "items": data,
                "total": total,
                "page": page,
                "page_size": page_size,
                "total_pages": (total + page_size - 1) / page_size
            }),
            "获取端口列表成功",
        ))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("获取端口列表失败: {}", e)))),
    }
}

pub async fn get_all_switch_ports(
    pool: web::Data<DbPool>,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse> {
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = query
        .get("page_size")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let search = query.get("search").cloned().unwrap_or_default();
    let offset = (page - 1) * page_size;

    let search_pattern = if !search.is_empty() {
        Some(format!("%{}%", search))
    } else {
        None
    };

    let total: i64 = match if let Some(ref pattern) = search_pattern {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM switch_ports sp JOIN switches s ON sp.switch_id = s.id WHERE s.name ILIKE $1 OR sp.port_number::TEXT ILIKE $1 OR sp.port_name ILIKE $1 OR sp.description ILIKE $1"
        )
        .bind(pattern)
        .fetch_one(pool.get_conn())
        .await
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM switch_ports sp JOIN switches s ON sp.switch_id = s.id",
        )
        .fetch_one(pool.get_conn())
        .await
    } {
        Ok(t) => t,
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("获取端口数量失败: {}", e))));
        }
    };

    let ports = if let Some(ref pattern) = search_pattern {
        sqlx::query_as::<_, SwitchPortWithSwitch>(
            r#"SELECT 
                sp.id, sp.switch_id, s.name as switch_name, 
                COALESCE(
                    (SELECT host(im.ip_address) FROM ip_managers im WHERE im.switch_id = s.id LIMIT 1),
                    ''
                ) as switch_ip,
                sp.port_number, sp.port_name, sp.port_type, sp.vlan_id,
                sp.status, sp.speed, sp.description, sp.created_at, sp.updated_at
            FROM switch_ports sp
            JOIN switches s ON sp.switch_id = s.id
            WHERE s.name ILIKE $1 OR sp.port_number::TEXT ILIKE $1 OR sp.port_name ILIKE $1 OR sp.description ILIKE $1
            ORDER BY s.name, sp.port_number
            LIMIT $2 OFFSET $3"#
        )
        .bind(pattern)
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
    } else {
        sqlx::query_as::<_, SwitchPortWithSwitch>(
            r#"SELECT 
                sp.id, sp.switch_id, s.name as switch_name, 
                COALESCE(
                    (SELECT host(im.ip_address) FROM ip_managers im WHERE im.switch_id = s.id LIMIT 1),
                    ''
                ) as switch_ip,
                sp.port_number, sp.port_name, sp.port_type, sp.vlan_id,
                sp.status, sp.speed, sp.description, sp.created_at, sp.updated_at
            FROM switch_ports sp
            JOIN switches s ON sp.switch_id = s.id
            ORDER BY s.name, sp.port_number
            LIMIT $1 OFFSET $2"#
        )
        .bind(page_size)
        .bind(offset)
        .fetch_all(pool.get_conn())
        .await
    };

    match ports {
        Ok(data) => Ok(HttpResponse::Ok().json(ApiResponse::success(
            serde_json::json!({
                "items": data,
                "total": total,
                "page": page,
                "page_size": page_size,
                "total_pages": (total + page_size - 1) / page_size
            }),
            "获取所有端口列表成功",
        ))),
        Err(e) => Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "获取所有端口列表失败: {}",
                e
            ))),
        ),
    }
}

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

    let switch_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM switches WHERE id = $1)")
            .bind(switch_id)
            .fetch_one(pool.get_conn())
            .await
            .unwrap_or(false);

    if !switch_exists {
        return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
    }

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

pub async fn get_switch_port(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let port_id = path.into_inner();

    let port = sqlx::query_as::<_, SwitchPortWithSwitch>(
        r#"SELECT 
            sp.id, sp.switch_id, s.name as switch_name, 
            COALESCE(
                (SELECT host(im.ip_address) FROM ip_managers im WHERE im.switch_id = s.id LIMIT 1),
                ''
            ) as switch_ip,
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

pub async fn delete_switch_port(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    http_req: HttpRequest,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    let port_id = path.into_inner();

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

    let has_cabinet_position = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM position_ports WHERE switch_port_id = $1)",
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

pub async fn sync_ports_from_snmp(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let switch_data = sqlx::query_as::<_, SwitchForSnmp>(
        r#"SELECT 
            id, name, snmp_version, snmp_community, 
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password, snmp_priv_protocol, 
            snmp_priv_password, snmp_port
        FROM switches WHERE id = $1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool.get_conn())
    .await;

    let switch_data = match switch_data {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在")));
        }
        Err(e) => {
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("查询交换机失败: {}", e))));
        }
    };

    let ip_address: Option<String> = sqlx::query_scalar(
        r#"SELECT host(ip_address) FROM ip_managers 
           WHERE switch_id = $1 AND device_type = 'switch' 
           ORDER BY created_at LIMIT 1"#,
    )
    .bind(switch_id)
    .fetch_optional(pool.get_conn())
    .await
    .ok()
    .flatten();

    let ip_address = match ip_address {
        Some(ref ip) if !ip.is_empty() => ip,
        _ => {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error("交换机没有配置IP地址"))
            );
        }
    };

    let snmp_params = switch_data.to_snmp_params(ip_address);

    let ports = match get_switch_ports_via_snmp(&snmp_params).await {
        Ok(p) => p,
        Err(e) => {
            return Ok(
                HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                    "获取交换机端口信息失败: {}",
                    e
                ))),
            );
        }
    };

    let mut saved_count = 0;
    let mut skipped_count = 0;

    for port in &ports {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM switch_ports WHERE switch_id = $1 AND port_number = $2)",
        )
        .bind(switch_id)
        .bind(&port.port_number)
        .fetch_one(pool.get_conn())
        .await
        .unwrap_or(true);

        if exists {
            skipped_count += 1;
            continue;
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
        .bind(&port.port_number)
        .bind(&port.port_name)
        .bind(port.port_type.as_deref().unwrap_or("access"))
        .bind(port.vlan_id)
        .bind(port.status.as_deref().unwrap_or("up"))
        .bind(&port.speed)
        .bind(&port.description)
        .bind(now)
        .bind(now)
        .execute(pool.get_conn())
        .await;

        if result.is_ok() {
            saved_count += 1;
        }
    }

    let saved_ports = sqlx::query_as::<_, SwitchPort>(
        "SELECT * FROM switch_ports WHERE switch_id = $1 ORDER BY port_number",
    )
    .bind(switch_id)
    .fetch_all(pool.get_conn())
    .await
    .unwrap_or_default();

    let message = if saved_count > 0 && skipped_count > 0 {
        format!(
            "成功保存 {} 个端口，跳过 {} 个已存在的端口",
            saved_count, skipped_count
        )
    } else if saved_count > 0 {
        format!("成功保存 {} 个端口到数据库", saved_count)
    } else if skipped_count > 0 {
        format!("所有 {} 个端口已存在，跳过保存", skipped_count)
    } else {
        "未获取到端口信息".to_string()
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(saved_ports, &message)))
}
