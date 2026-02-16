pub mod notification;

use crate::db::DbPool;
use crate::models::{ApiResponse, LoginLog, OperationLog};
use actix_web::{HttpResponse, Result, web};
use tracing_subscriber::prelude::*;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::LocalTime;
use std::path::Path;
use std::fs;

// 获取操作日志（支持搜索和分页）
pub async fn get_operation_logs(
    pool: web::Data<DbPool>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let resource_type = query.get("resource_type").map(|s| s.as_str()).unwrap_or("");
    let resource_id = query.get("resource_id").map(|s| s.as_str()).unwrap_or("");
    let user_id = query.get("user_id").map(|s| s.as_str()).unwrap_or("");
    let action = query.get("action").map(|s| s.as_str()).unwrap_or("");
    let page: i64 = query.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let page_size: i64 = query.get("page_size").and_then(|s| s.parse().ok()).unwrap_or(50);
    let offset = (page - 1) * page_size;

    let mut conditions = Vec::new();

    if !resource_type.is_empty() {
        conditions.push(format!("ol.resource_type = '{}'", resource_type.replace("'", "''")));
    }

    if !resource_id.is_empty() {
        conditions.push(format!("ol.resource_id = '{}'", resource_id.replace("'", "''")));
    }

    if !user_id.is_empty() {
        conditions.push(format!("ol.user_id = '{}'", user_id.replace("'", "''")));
    }

    if !action.is_empty() {
        let search_term = action.replace("'", "''");
        conditions.push(format!(
            "(ol.action ILIKE '%{}%' OR u.username ILIKE '%{}%' OR ol.ip_address ILIKE '%{}%' OR ol.resource_type ILIKE '%{}%' OR ol.resource_id::TEXT ILIKE '%{}%')",
            search_term, search_term, search_term, search_term, search_term
        ));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_query = format!(
        "SELECT COUNT(*) FROM operation_logs ol {}",
        where_clause
    );
    let total: i64 = match sqlx::query_scalar(&count_query)
        .fetch_one(pool.get_conn())
        .await
    {
        Ok(count) => count,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "数据库查询错误: {}",
                err
            ))));
        }
    };

    let data_query = format!(
        "SELECT ol.id, ol.user_id, COALESCE(u.username, '已删除用户') as username, ol.action, ol.action as operation_type, ol.resource_type, ol.resource_id, ol.details, ol.result, ol.ip_address, ol.created_at::TIMESTAMPTZ FROM operation_logs ol LEFT JOIN users u ON ol.user_id = u.id {} ORDER BY ol.created_at DESC LIMIT {} OFFSET {}",
        where_clause, page_size, offset
    );

    let logs = match sqlx::query_as::<_, OperationLog>(&data_query)
        .fetch_all(pool.get_conn())
        .await
    {
        Ok(logs) => logs,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "数据库查询错误: {}",
                err
            ))));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "data": logs,
            "total": total,
            "page": page,
            "page_size": page_size,
            "total_pages": (total + page_size - 1) / page_size
        }),
        "操作日志获取成功",
    )))
}

// 获取登录日志
pub async fn get_login_logs(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let logs = match sqlx::query_as::<_, LoginLog>(
        "SELECT id, username, CAST(ip_address AS TEXT) as ip_address, user_agent, success, error_message, created_at::TIMESTAMPTZ FROM login_logs ORDER BY created_at DESC"
    ).fetch_all(pool.get_conn()).await {
        Ok(logs) => logs,
        Err(err) => {
            return Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!("Database query error: {}", err))));
        }
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::<Vec<LoginLog>>::success(
            logs,
            "登录日志获取成功",
        )),
    )
}

pub fn setup_logging() -> String {
    // 初始化日志 - 使用本地时间
    let timer = LocalTime::new(time::format_description::parse(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]"
    ).unwrap());

    // 获取程序名
    let app_name = env!("CARGO_PKG_NAME");
    // 构建日志目录路径
    let log_dir = format!("/var/log/{}", app_name);
    // 创建日志目录（如果不存在）
    if !Path::new(&log_dir).exists() {
        fs::create_dir_all(&log_dir).unwrap_or_else(|e| {
            eprintln!("创建日志目录失败: {}", e);
        });
    }
    // 构建日志文件路径，格式：/var/log/程序名/日期.log
    // 使用本地时间（CST）而不是UTC时间
    let today = chrono::Local::now().format("%Y-%m-%d-%H-%M").to_string();
    let log_file_path = format!("{}/{}.log", log_dir, today);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(timer.clone()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::fs::File::create(&log_file_path).unwrap_or_else(|e| {
                    eprintln!("创建日志文件失败: {}", e);
                    // 如果创建文件失败，回退到标准输出
                    std::fs::File::create("ipma.log").unwrap()
                }))
                .with_timer(timer)
                .with_ansi(false), // 禁用ANSI颜色代码
        )
        .with(
            tracing_subscriber::filter::Targets::new()
                .with_target("ipma", LevelFilter::INFO)
                .with_target("actix_web", LevelFilter::WARN)
                .with_default(LevelFilter::WARN),
        )
        .init();

    log_file_path
}
