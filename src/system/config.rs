use crate::config::{Config, I18nConfig, ServerConfig};
use crate::db::DbPool;
use crate::models::ApiResponse;
use crate::system::smtp::SmtpConfig;
use crate::system::smtp::{
    get_smtp_config_from_db, save_smtp_config_to_db, send_email_to_users,
    test_smtp_connection as test_smtp_connection_impl,
};
use actix_web::{HttpResponse, Result, web};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use validator::Validate;

// 系统启动时间（秒）
static START_TIME: AtomicU64 = AtomicU64::new(0);

// SMTP邮件请求模型
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendEmailRequest {
    #[validate(length(min = 1, message = "收件人不能为空"))]
    pub user_ids: Vec<Uuid>,
    #[validate(length(min = 1, max = 255, message = "主题长度必须在1到255个字符之间"))]
    pub subject: String,
    #[validate(length(min = 1, message = "邮件内容不能为空"))]
    pub body: String,
    pub is_html: Option<bool>,
}

// SMTP配置更新请求模型
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSmtpConfigRequest {
    #[validate(length(min = 1, message = "SMTP服务器地址不能为空"))]
    pub host: String,
    pub port: u16,
    #[validate(length(min = 1, message = "SMTP用户名不能为空"))]
    pub username: String,
    #[validate(length(min = 1, message = "SMTP密码不能为空"))]
    pub password: String,
    #[validate(email(message = "请输入有效的发件人邮箱地址"))]
    pub from: String,
    pub secure: bool,
}

// 系统信息模型
#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub name: String,
    pub version: String,
    pub uptime: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub database_status: String,
    pub config: SafeConfig,
}

#[derive(Debug, Serialize)]
pub struct SafeDatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub max_connections: u32,
    pub query_timeout_secs: u64,
    pub slow_query_threshold_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct SafeJwtConfig {
    pub access_token_expiry: String,
    pub refresh_token_expiry: String,
}

#[derive(Debug, Serialize)]
pub struct SafeConfig {
    pub database: SafeDatabaseConfig,
    pub server: ServerConfig,
    pub jwt: SafeJwtConfig,
    pub init: crate::config::InitConfig,
    pub i18n: Option<I18nConfig>,
    pub rate_limit: crate::config::RateLimitConfig,
}

impl From<Config> for SafeConfig {
    fn from(config: Config) -> Self {
        Self {
            database: SafeDatabaseConfig {
                host: config.database.host,
                port: config.database.port,
                database: config.database.database,
                username: config.database.username,
                max_connections: config.database.max_connections,
                query_timeout_secs: config.database.query_timeout_secs,
                slow_query_threshold_ms: config.database.slow_query_threshold_ms,
            },
            server: config.server,
            jwt: SafeJwtConfig {
                access_token_expiry: config.jwt.access_token_expiry,
                refresh_token_expiry: config.jwt.refresh_token_expiry,
            },
            init: config.init,
            i18n: config.i18n,
            rate_limit: config.rate_limit,
        }
    }
}

// 系统配置更新请求模型
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSystemConfigRequest {
    #[serde(default)]
    pub database: Option<crate::config::DatabaseConfig>,
    #[serde(default)]
    pub server: Option<crate::config::ServerConfig>,
    #[serde(default)]
    pub jwt: Option<crate::config::JwtConfig>,
    #[serde(default)]
    pub init: Option<crate::config::InitConfig>,
    #[serde(default)]
    pub rate_limit: Option<crate::config::RateLimitConfig>,
    #[serde(default)]
    pub snmp: Option<crate::config::SnmpConfig>,
}

pub fn init_start_time() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            tracing::warn!("初始化系统启动时间失败: {}", e);
            0
        });
    START_TIME.store(now, Ordering::SeqCst);
}

// 保存配置到文件
fn save_config_to_file(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = crate::config::get_config_file_path();
    tracing::info!("[save_config] 开始保存配置到: {}", config_path);

    let toml_str = toml::to_string_pretty(config)?;
    tracing::info!("[save_config] TOML 内容长度: {}", toml_str.len());

    std::fs::write(&config_path, &toml_str)?;
    tracing::info!("[save_config] 配置已写入文件");

    // 验证写入
    let verify_content = std::fs::read_to_string(&config_path)?;
    tracing::info!(
        "[save_config] 验证读取成功，内容长度: {}",
        verify_content.len()
    );

    Ok(())
}

// 获取系统信息
pub async fn get_system_info(
    pool: web::Data<DbPool>,
    _config: web::Data<Config>,
) -> Result<HttpResponse> {
    let database_status = match sqlx::query("SELECT 1").execute(&pool.get_conn()).await {
        Ok(_) => "connected".to_string(),
        Err(e) => {
            tracing::error!("数据库连接检查失败: {}", e);
            format!("disconnected: {}", e)
        }
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            tracing::warn!("系统时间异常: {}", e);
            0
        });
    let start_time = START_TIME.load(Ordering::SeqCst);
    let uptime = if start_time == 0 {
        init_start_time();
        0
    } else if start_time > now {
        tracing::warn!("系统启动时间晚于当前时间，时间可能已被调整");
        0
    } else {
        now - start_time
    };

    // 从文件加载最新配置
    let latest_config = Config::load().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to load latest config: {e}"))
    })?;

    // 创建系统信息响应
    let system_info = SystemInfo {
        name: "IPMA".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime,
        timestamp: chrono::Utc::now(),
        database_status: database_status.to_string(),
        config: SafeConfig::from(latest_config),
    };

    Ok(HttpResponse::Ok().json(ApiResponse::<SystemInfo>::success(
        system_info,
        "系统信息获取成功",
    )))
}

// 获取仪表盘统计数据
type CountStats = (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64);

pub async fn get_dashboard_stats(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let conn = pool.get_conn();

    let stats_row1: sqlx::Result<CountStats> =
        sqlx::query_as(
            r"SELECT
                (SELECT COUNT(*) FROM users) AS total_users,
                (SELECT COUNT(*) FROM users WHERE status = true) AS active_users,
                (SELECT COUNT(*) FROM network_regions) AS total_network_regions,
                (SELECT COUNT(*) FROM network_cidrs) AS total_networks,
                (SELECT COUNT(*) FROM rooms) AS total_rooms,
                (SELECT COUNT(*) FROM cabinets) AS total_cabinets,
                (SELECT COUNT(*) FROM workstations) AS total_workstations,
                (SELECT COUNT(*) FROM positions) AS total_positions,
                (SELECT COUNT(*) FROM switches) AS total_switches,
                (SELECT COUNT(*) FROM ips) AS total_ips
            ",
        )
        .fetch_one(&conn)
        .await;

    let stats_row2: sqlx::Result<(i64, i64, i64)> = sqlx::query_as(
        r"SELECT
            (SELECT COUNT(*) FROM ips WHERE status = 'active') AS active_ips,
            (SELECT COUNT(*) FROM operation_logs WHERE created_at > NOW() - INTERVAL '24 hours') AS recent_logs,
            (SELECT COUNT(*) FROM login_logs WHERE created_at > NOW() - INTERVAL '24 hours') AS login_logs_today
        ",
    )
    .fetch_one(&conn)
    .await;

    let (ips_by_device_type, ips_by_status, rooms_by_type) = tokio::join!(
        sqlx::query_as::<_, (String, i64)>(
            "SELECT device_type, COUNT(*) as count FROM ips WHERE device_type IS NOT NULL GROUP BY device_type"
        )
        .fetch_all(&conn),
        sqlx::query_as::<_, (String, i64)>(
            "SELECT status, COUNT(*) as count FROM ips GROUP BY status"
        )
        .fetch_all(&conn),
        sqlx::query_as::<_, (String, i64)>(
            "SELECT room_type, COUNT(*) as count FROM rooms GROUP BY room_type"
        )
        .fetch_all(&conn),
    );

    let (
        total_users,
        active_users,
        total_network_regions,
        total_networks,
        total_rooms,
        total_cabinets,
        total_workstations,
        total_positions,
        total_switches,
        total_ips,
    ) = match stats_row1 {
        Ok(stats) => stats,
        Err(e) => {
            tracing::error!("获取仪表盘统计失败: {}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("获取统计数据失败: {}", e))));
        }
    };

    let (active_ips, recent_logs, login_logs_today) = match stats_row2 {
        Ok(stats) => stats,
        Err(e) => {
            tracing::error!("获取活跃IP统计失败: {}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("获取活跃IP统计失败: {}", e))));
        }
    };

    let ips_by_device_type = match ips_by_device_type {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("获取IP设备类型统计失败: {}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("获取IP设备类型统计失败: {}", e))));
        }
    };
    let ips_by_status = match ips_by_status {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("获取IP状态统计失败: {}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("获取IP状态统计失败: {}", e))));
        }
    };
    let rooms_by_type = match rooms_by_type {
        Ok(data) => data,
        Err(e) => {
            tracing::error!("获取房间类型统计失败: {}", e);
            return Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("获取房间类型统计失败: {}", e))));
        }
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "users": {
                "total": total_users,
                "active": active_users
            },
            "networks": {
                "regions": total_network_regions,
                "networks": total_networks
            },
            "locations": {
                "rooms": total_rooms,
                "cabinets": total_cabinets,
                "workstations": total_workstations,
                "positions": total_positions
            },
            "switches": total_switches,
            "ips": {
                "total": total_ips,
                "active": active_ips,
                "by_device_type": ips_by_device_type.into_iter().collect::<std::collections::HashMap<_, _>>(),
                "by_status": ips_by_status.into_iter().collect::<std::collections::HashMap<_, _>>()
            },
            "rooms_by_type": rooms_by_type.into_iter().collect::<std::collections::HashMap<_, _>>(),
            "activity": {
                "operations_24h": recent_logs,
                "logins_24h": login_logs_today
            }
        }),
        "统计数据获取成功",
    )))
}

// 更新系统配置
pub async fn update_system_config(
    req: web::Json<UpdateSystemConfigRequest>,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    // 创建新的配置对象，只更新提供的字段
    tracing::info!("[update_config] 请求数据: {:?}", req);

    let new_config = Config {
        database: req
            .database
            .clone()
            .unwrap_or_else(|| config.database.clone()),
        server: req.server.clone().unwrap_or_else(|| config.server.clone()),
        jwt: req.jwt.clone().unwrap_or_else(|| config.jwt.clone()),
        init: req.init.clone().unwrap_or_else(|| config.init.clone()),
        i18n: config.i18n.clone(),
        rate_limit: req
            .rate_limit
            .clone()
            .unwrap_or_else(|| config.rate_limit.clone()),
        snmp: req.snmp.clone().unwrap_or_else(|| config.snmp.clone()),
    };

    tracing::info!(
        "[update_config] 新的 rate_limit: {:?}",
        new_config.rate_limit
    );

    // 保存配置到文件
    let config_path = crate::config::get_config_file_path();
    tracing::info!("[update_config] 准备保存配置到: {}", config_path);

    match save_config_to_file(&new_config) {
        Ok(()) => {
            tracing::info!("配置已保存到: {}", config_path);
            Ok(HttpResponse::Ok().json(ApiResponse::success(new_config, "配置更新成功")))
        }
        Err(e) => {
            tracing::error!("配置保存失败: {}", e);
            Ok(HttpResponse::InternalServerError()
                .json(ApiResponse::<()>::error(format!("配置保存失败: {e:?}"))))
        }
    }
}

// 统一的服务重启函数
pub fn trigger_service_restart() -> Result<HttpResponse> {
    let service_name = "ipma.service";

    let is_running_as_service = check_if_running_as_service();

    tracing::info!(
        "触发服务重启, is_running_as_service: {}",
        is_running_as_service
    );

    if is_running_as_service {
        // 先尝试 systemctl restart（检查权限）
        let check_output = Command::new("systemctl")
            .args(["show", "ipma.service", "--property=ActiveState"])
            .output();

        let is_active = match check_output {
            Ok(output) => {
                let status = String::from_utf8_lossy(&output.stdout);
                status.contains("ActiveState=active")
            }
            Err(_) => true,
        };

        tracing::info!("服务当前状态: active={}", is_active);

        // 尝试重启
        let output = Command::new("systemctl")
            .arg("restart")
            .arg(service_name)
            .output();

        match output {
            Ok(output) => {
                let exit_code = output.status.code().unwrap_or(-1);
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                tracing::info!(
                    "systemctl restart 执行完成: exit_code={}, stdout='{}', stderr='{}'",
                    exit_code,
                    stdout.trim(),
                    stderr.trim()
                );

                // exit code 0 表示成功
                if output.status.success() {
                    tracing::info!("systemctl restart 执行成功");
                    return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
                        (),
                        "服务重启命令已发送，服务正在重启...",
                    )));
                }

                // 非 0 退出码，检查具体错误
                if stderr.contains("Access denied")
                    || stderr.contains("Permission denied")
                    || stderr.contains("Interactive authentication required")
                {
                    tracing::info!("权限不足，使用进程退出方式重启");
                    return restart_by_exit();
                }

                // 如果 stderr 包含 "Failed"，说明真的失败了
                if stderr.contains("Failed") && !stderr.contains("Failed to restart") {
                    tracing::error!("服务重启失败: {}", stderr);
                    return Ok(
                        HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                            format!("服务重启失败: {}", stderr.trim()),
                        )),
                    );
                }

                // 其他情况，可能是警告，尝试退出方式
                tracing::info!("systemctl 返回非零状态码，使用进程退出方式重启");
                restart_by_exit()
            }
            Err(e) => {
                tracing::warn!("systemctl restart 执行失败: {}，使用退出方式重启", e);
                restart_by_exit()
            }
        }
    } else {
        tracing::info!("非服务模式运行，使用独立进程重启");
        restart_standalone_process()
    }
}

// 重启应用系统（API 端点）
pub async fn restart_application() -> Result<HttpResponse> {
    tracing::info!("收到重启应用请求");
    trigger_service_restart()
}

fn restart_by_exit() -> Result<HttpResponse> {
    tracing::info!("使用进程退出方式触发重启（systemd Restart=always 会自动重启）");

    // 先返回响应，然后在后台线程中退出
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(500));
        tracing::info!("程序即将退出，等待 systemd 自动重启...");
        std::process::exit(0);
    });

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "服务正在重启...")))
}

fn check_if_running_as_service() -> bool {
    if std::env::var("INVOCATION_ID").is_ok() {
        return true;
    }

    if let Ok(pid) = std::fs::read_to_string("/proc/self/cgroup")
        && (pid.contains("systemd") || pid.contains(".service"))
    {
        return true;
    }

    std::path::Path::new("/etc/systemd/system/ipma.service").exists()
}

fn restart_standalone_process() -> Result<HttpResponse> {
    let exe_path = std::env::current_exe().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取可执行文件路径失败: {e}"))
    })?;

    let exe_path_str = exe_path.to_str().ok_or_else(|| {
        actix_web::error::ErrorInternalServerError("无法将可执行文件路径转换为字符串")
    })?;

    let working_dir = std::env::current_dir().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取工作目录失败: {e}"))
    })?;

    let working_dir_str = working_dir.to_str().ok_or_else(|| {
        actix_web::error::ErrorInternalServerError("无法将工作目录路径转换为字符串")
    })?;

    let restart_script = format!(
        r#"#!/bin/bash
sleep 3
cd "{working_dir_str}"
exec "{exe_path_str}"
"#
    );

    let script_path = "/tmp/ipma_restart.sh";
    std::fs::write(script_path, restart_script).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("创建重启脚本失败: {e}"))
    })?;

    let output = Command::new("chmod")
        .arg("+x")
        .arg(script_path)
        .output()
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("设置脚本权限失败: {e}"))
        })?;

    if !output.status.success() {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error("设置脚本权限失败"))
        );
    }

    if let Err(e) = Command::new("nohup").arg(script_path).spawn() {
        tracing::warn!("启动重启脚本失败: {}", e);
    }

    // 延迟退出，确保HTTP响应能成功发送给客户端
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(2));
        std::process::exit(0);
    });

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "服务重启命令已发送，服务正在重启",
    )))
}

// 重启操作系统
pub async fn restart_os() -> Result<HttpResponse> {
    // 执行系统重启逻辑
    let output = Command::new("sh")
        .arg("-c")
        .arg("sleep 2 && sudo reboot")
        .output()
        .map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("执行重启命令失败: {e}"))
        })?;

    if !output.status.success() {
        let error_message = String::from_utf8_lossy(&output.stderr);
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "操作系统重启失败: {error_message}"
            ))),
        );
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "操作系统重启命令已发送，系统将在2秒后重启",
    )))
}

// 检查服务状态
pub async fn check_service_status() -> Result<HttpResponse> {
    // 检查服务文件路径（deb 包和手动注册）
    let service_paths = [
        "/usr/lib/systemd/system/ipma.service", // deb 包安装的路径
        "/etc/systemd/system/ipma.service",     // 手动注册的路径
    ];

    let mut service_file_exists = false;
    let mut service_file_path = String::new();

    for path in &service_paths {
        if std::path::Path::new(path).exists() {
            service_file_exists = true;
            service_file_path = path.to_string();
            break;
        }
    }

    let mut is_enabled = false;
    let mut is_active = false;
    let mut status_text = "未安装".to_string();
    let mut uptime_seconds: Option<u64> = None;

    if service_file_exists {
        let enable_output = Command::new("systemctl")
            .arg("is-enabled")
            .arg("ipma.service")
            .output()
            .ok();

        if let Some(output) = enable_output {
            is_enabled = output.status.success();
        }

        let active_output = Command::new("systemctl")
            .arg("is-active")
            .arg("ipma.service")
            .output()
            .ok();

        if let Some(output) = active_output {
            is_active = output.status.success();
            status_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }

        if is_active {
            let show_output = Command::new("systemctl")
                .args(["show", "ipma.service", "--property=ExecMainStartTimestamp"])
                .output()
                .ok();

            if let Some(output) = show_output {
                let prop = String::from_utf8_lossy(&output.stdout);
                if let Some(timestamp_str) = prop.strip_prefix("ExecMainStartTimestamp=") {
                    let timestamp_str = timestamp_str.trim();
                    if !timestamp_str.is_empty()
                        && timestamp_str != "n/a"
                        && let Ok(start_time) = chrono::DateTime::parse_from_rfc3339(timestamp_str)
                    {
                        let now = chrono::Utc::now();
                        uptime_seconds = Some(
                            (now - start_time.with_timezone(&chrono::Utc)).num_seconds() as u64,
                        );
                    }
                }
            }
        }
    }

    let running_as_service = check_if_running_as_service();

    let status_data = serde_json::json!({
        "registered": service_file_exists && is_enabled,
        "service_file_exists": service_file_exists,
        "service_file_path": service_file_path,
        "enabled": is_enabled,
        "active": is_active,
        "status": status_text,
        "running_as_service": running_as_service,
        "uptime_seconds": uptime_seconds
    });

    tracing::info!(
        "服务状态: registered={}, enabled={}, active={}, running_as_service={}",
        service_file_exists && is_enabled,
        is_enabled,
        is_active,
        running_as_service
    );

    Ok(HttpResponse::Ok().json(ApiResponse::success(status_data, "服务状态检查成功")))
}

// 注册为服务
pub async fn register_service() -> Result<HttpResponse> {
    tracing::info!("收到注册服务请求");

    // 检查服务是否已存在
    let service_paths = [
        "/usr/lib/systemd/system/ipma.service", // deb 包安装的路径
        "/etc/systemd/system/ipma.service",     // 手动注册的路径
    ];

    for path in &service_paths {
        if std::path::Path::new(path).exists() {
            // 检查服务是否已启用
            let is_enabled = Command::new("systemctl")
                .args(["is-enabled", "ipma.service"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            tracing::info!("服务文件已存在: {}, 已启用: {}", path, is_enabled);

            return Ok(HttpResponse::Ok().json(ApiResponse::success(
                serde_json::json!({
                    "registered": true,
                    "service_file": path,
                    "enabled": is_enabled,
                    "message": "服务已注册"
                }),
                "服务已注册",
            )));
        }
    }

    // 服务不存在，尝试注册
    tracing::info!("服务文件不存在，尝试创建服务文件");

    // 获取当前可执行文件路径
    let exe_path = std::env::current_exe().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("获取当前可执行文件路径失败: {e}"))
    })?;
    let exe_path_str = exe_path.to_str().ok_or_else(|| {
        actix_web::error::ErrorInternalServerError("无法将可执行文件路径转换为字符串")
    })?;

    // 获取配置文件路径
    let config_path = crate::config::get_config_file_path();
    let config_dir = std::path::Path::new(&config_path)
        .parent().map_or_else(|| "/etc/ipma".to_string(), |p| p.to_string_lossy().to_string());

    // 创建 systemd 服务文件内容
    let service_content = format!(
        r"[Unit]
Description=IP Management Application
Documentation=man:ipma(1)
After=network.target postgresql.service
Wants=postgresql.service

[Service]
Type=simple
User=ipma
Group=ipma
WorkingDirectory=/opt/ipma
ExecStart={exe_path_str}
Restart=always
RestartSec=5s

KillSignal=SIGTERM
TimeoutStopSec=30s
KillMode=mixed

# 允许绑定低端口 (80, 443)
AmbientCapabilities=CAP_NET_BIND_SERVICE

# 安全加固
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/ipma /var/log/ipma {config_dir}
PrivateTmp=true

# 资源限制
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
"
    );

    // 尝试写入服务文件（需要权限）
    let write_result = Command::new("pkexec")
        .args(["--user", "root", "tee", "/etc/systemd/system/ipma.service"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let result = match write_result {
        Ok(mut child) => {
            // 写入服务文件内容
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take()
                && let Err(e) = stdin.write_all(service_content.as_bytes())
            {
                tracing::warn!("写入服务文件内容失败: {}", e);
            }
            child.wait_with_output()
        }
        Err(_) => {
            Command::new("sudo")
                .args(["tee", "/etc/systemd/system/ipma.service"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    use std::io::Write;
                    if let Some(mut stdin) = child.stdin.take()
                        && let Err(e) = stdin.write_all(service_content.as_bytes())
                    {
                        tracing::warn!("写入服务文件内容失败: {}", e);
                    }
                    child.wait_with_output()
                })
        }
    };

    match result {
        Ok(output) if output.status.success() => {
            tracing::info!("服务文件创建成功");

            // 执行后续命令
            let commands = vec![vec!["daemon-reload"], vec!["enable", "ipma.service"]];

            for args in commands {
                let cmd_result = execute_systemctl_with_privilege(&args);
                if let Err(e) = cmd_result {
                    tracing::warn!("执行 systemctl {:?} 失败: {}", args, e);
                }
            }

            Ok(HttpResponse::Ok().json(ApiResponse::success(
                serde_json::json!({
                    "registered": true,
                    "service_file": "/etc/systemd/system/ipma.service",
                    "enabled": true,
                    "message": "服务注册成功"
                }),
                "系统服务注册成功，IPMA服务已设置为开机自启",
            )))
        }
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            tracing::error!("创建服务文件失败: {}", error);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                    "创建服务文件失败，需要管理员权限: {error}"
                ))),
            )
        }
        Err(e) => {
            tracing::error!("执行命令失败: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "需要管理员权限来注册服务。请手动执行以下命令：\nsudo tee /etc/systemd/system/ipma.service <<< '{}'\nsudo systemctl daemon-reload\nsudo systemctl enable ipma.service",
                service_content.replace('\'', "'\\''")
            ))))
        }
    }
}

// 使用权限执行 systemctl 命令
fn execute_systemctl_with_privilege(args: &[&str]) -> Result<(), String> {
    // 首先尝试直接执行（可能有 polkit 权限）
    let direct_result = Command::new("systemctl").args(args).output();

    match direct_result {
        Ok(output) if output.status.success() => {
            return Ok(());
        }
        Ok(_) | Err(_) => {}
    }

    let pkexec_result = Command::new("pkexec")
        .args(["--user", "root", "systemctl"])
        .args(args)
        .output();

    match pkexec_result {
        Ok(output) if output.status.success() => {
            return Ok(());
        }
        Ok(_) | Err(_) => {}
    }

    // 尝试 sudo
    let sudo_result = Command::new("sudo").args(["systemctl"]).args(args).output();

    match sudo_result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(format!("systemctl {args:?} 失败: {error}"))
        }
        Err(e) => Err(format!("执行命令失败: {e}")),
    }
}

// 获取SMTP配置
pub async fn get_smtp_config(
    pool: web::Data<DbPool>,
    _config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 从数据库中获取SMTP配置
    let smtp_config = get_smtp_config_from_db(&pool.get_conn()).await;

    Ok(HttpResponse::Ok().json(ApiResponse::success(smtp_config, "SMTP配置获取成功")))
}

// 更新SMTP配置
pub async fn update_smtp_config(
    pool: web::Data<DbPool>,
    req: web::Json<UpdateSmtpConfigRequest>,
    _config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    // 构建SMTP配置对象
    let smtp_config = SmtpConfig {
        host: req.host.clone(),
        port: req.port,
        username: req.username.clone(),
        password: req.password.clone(),
        from: req.from.clone(),
        secure: req.secure,
    };

    // 保存SMTP配置到数据库
    if let Err(e) = save_smtp_config_to_db(&pool.get_conn(), &smtp_config).await {
        return Ok(
            HttpResponse::InternalServerError().json(ApiResponse::<()>::error(format!(
                "保存SMTP配置失败: {e:?}"
            ))),
        );
    }

    // 记录SMTP配置更新日志
    if let Err(e) = sqlx::query(
        "INSERT INTO task_logs (id, task_name, status, details, start_time, end_time) 
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(uuid::Uuid::new_v4())
    .bind("update_smtp_config")
    .bind("success")
    .bind(sqlx::types::Json(serde_json::json!({
        "host": req.host,
        "port": req.port,
        "username": req.username,
        "from": req.from,
        "secure": req.secure
    })))
    .bind(chrono::Utc::now())
    .bind(chrono::Utc::now())
    .execute(&pool.get_conn())
    .await {
        tracing::warn!("记录操作日志失败: {}", e);
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "SMTP配置更新成功")))
}

// 测试SMTP连接
pub async fn test_smtp_connection(
    pool: web::Data<DbPool>,
    req: web::Json<UpdateSmtpConfigRequest>,
) -> Result<HttpResponse> {
    // 验证请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    // 记录SMTP连接测试日志
    let task_id = Uuid::new_v4();
    let start_time = chrono::Utc::now();

    // 记录测试开始日志
    if let Err(e) = sqlx::query(
        "INSERT INTO task_logs (id, task_name, status, details, start_time) 
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(task_id)
    .bind("test_smtp_connection")
    .bind("running")
    .bind(sqlx::types::Json(serde_json::json!({
        "host": req.host,
        "port": req.port,
        "username": req.username,
        "from": req.from,
        "secure": req.secure
    })))
    .bind(start_time)
    .execute(&pool.get_conn())
    .await {
        tracing::warn!("记录操作日志失败: {}", e);
    }

    // 构建SMTP配置对象
    let smtp_config = SmtpConfig {
        host: req.host.clone(),
        port: req.port,
        username: req.username.clone(),
        password: req.password.clone(),
        from: req.from.clone(),
        secure: req.secure,
    };

    // 实际测试SMTP连接
    let test_result = test_smtp_connection_impl(&smtp_config).await;

    // 记录测试结果日志
    let end_time = chrono::Utc::now();
    let duration = end_time.timestamp() - start_time.timestamp();

    let (status, details) = if let Err(e) = &test_result {
        ("failed", serde_json::json!({ "error": e.to_string() }))
    } else {
        (
            "success",
            serde_json::json!({ "message": "SMTP连接测试成功" }),
        )
    };

    if let Err(e) = sqlx::query("UPDATE task_logs SET status = $1, end_time = $2, duration = $3, details = $4 WHERE id = $5")
        .bind(status)
        .bind(end_time)
        .bind(duration as i32)
        .bind(sqlx::types::Json(details))
        .bind(task_id)
        .execute(&pool.get_conn())
        .await
    {
        tracing::warn!("更新SMTP测试日志失败: {}", e);
    }

    match test_result {
        Ok(()) => Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "SMTP连接测试成功"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("SMTP连接测试失败: {e}")))),
    }
}

// 发送邮件
pub async fn send_email(
    pool: web::Data<DbPool>,
    req: web::Json<SendEmailRequest>,
    _config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    // 实际发送邮件
    let send_result: anyhow::Result<()> =
        send_email_to_users(&pool.get_conn(), &req.user_ids, &req.subject, &req.body).await;

    // 记录操作日志
    let end_time = chrono::Utc::now();
    let result = send_result.is_ok();
    let details = serde_json::json!({
        "user_ids": req.user_ids,
        "subject": req.subject,
        "error": send_result.as_ref().err().map(|e: &anyhow::Error| e.to_string())
    });

    if let Err(e) = sqlx::query(
        "INSERT INTO operation_logs (id, user_id, action, resource_type, resource_id, details, result, ip_address, created_at) 
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::nil())
    .bind("send_email")
    .bind("email")
    .bind(Uuid::nil())
    .bind(sqlx::types::Json(details))
    .bind(result)
    .bind("system")
    .bind(end_time)
    .execute(&pool.get_conn())
    .await {
        tracing::warn!("记录操作日志失败: {}", e);
    }

    match send_result {
        Ok(()) => Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "邮件发送成功"))),
        Err(e) => Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("邮件发送失败: {e}")))),
    }
}

// 更新系统配置
pub async fn update_config(
    _pool: web::Data<DbPool>,
    req: web::Json<serde_json::Value>,
) -> Result<HttpResponse> {
    let mut current_config = Config::load().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to load current config: {e}"))
    })?;

    if let Some(server_value) = req.get("server")
        && let Ok(server_config) = serde_json::from_value::<ServerConfig>(server_value.clone())
    {
        let http_enabled = server_config.http_enabled.unwrap_or(false);
        let https_enabled = server_config.https_enabled.unwrap_or(false);

        if !http_enabled && !https_enabled {
            return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
                "至少需要开启一个端口（HTTP或HTTPS）",
            )));
        }

        current_config.server = server_config;
    }

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to serialize config: {e}"))
    })?;

    tokio::fs::write(&config_path, config_str).await.map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to write config file: {e}"))
    })?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "系统配置更新成功")))
}

// 检查证书状态
#[derive(Debug, Serialize)]
pub struct CertificateStatus {
    pub has_imported_cert: bool,
    pub has_self_signed_cert: bool,
    pub cert_type: String,
}

// 辅助函数：检查是否存在指定类型的证书
fn check_certificate_exists(dir: &str, cert_type: &str) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename_os) = path.file_name()
                && let Some(filename) = filename_os.to_str()
                && ((cert_type == "import" && filename.starts_with("import_"))
                    || (cert_type == "create" && filename.starts_with("create_")))
            {
                return true;
            }
        }
    }
    false
}

pub async fn get_certificate_status() -> Result<HttpResponse> {
    // 检查是否存在导入的证书
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    let has_imported_cert = check_certificate_exists(&certs_dir, "import");

    // 检查是否存在自签名证书
    let has_self_signed_cert = check_certificate_exists(&certs_dir, "create");

    // 加载配置获取证书类型
    let cert_type = match Config::load() {
        Ok(config) => config
            .server
            .cert_type
            .unwrap_or_else(|| "self_signed".to_string()),
        Err(_) => "self_signed".to_string(),
    };

    let status = CertificateStatus {
        has_imported_cert,
        has_self_signed_cert,
        cert_type,
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(status, "证书状态获取成功")))
}

// 生成自签名证书请求
#[derive(Debug, Deserialize, validator::Validate)]
pub struct GenerateCertRequest {
    #[validate(length(min = 1))]
    pub common_name: String,
    pub organization: Option<String>,
    pub organizational_unit: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub locality: Option<String>,
    pub validity: Option<u32>,
}

// 生成自签名证书
pub async fn generate_certificate(
    _pool: web::Data<DbPool>,
    req: web::Json<GenerateCertRequest>,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    // 确保证书目录存在
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    if !std::path::Path::new(&certs_dir).exists() {
        std::fs::create_dir_all(&certs_dir).map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("创建证书目录失败: {e}"))
        })?;
    }

    // 生成带时间戳的证书文件名
    let timestamp = chrono::Utc::now().timestamp();
    let base_name = format!("create_{timestamp}_cert");
    let cert_path = format!("{certs_dir}/{base_name}.pem");
    let key_path = format!("{certs_dir}/{base_name}.key");

    if let Err(e) = generate_self_signed_cert(&cert_path, &key_path, &req, &config) {
        return Ok(HttpResponse::InternalServerError()
            .json(ApiResponse::<()>::error(format!("生成证书失败: {e:?}"))));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "证书生成成功")))
}

// 导入证书
pub async fn import_certificate(
    _pool: web::Data<DbPool>,
    req: actix_multipart::Multipart,
) -> Result<HttpResponse> {
    use futures_util::stream::StreamExt;

    let mut cert_data = Vec::new();
    let mut key_data = Vec::new();

    let mut multipart = req;

    while let Some(item) = multipart.next().await {
        if let Ok(mut field) = item {
            let name = field.name().unwrap_or("").to_string();

            while let Some(chunk) = field.next().await {
                if let Ok(data) = chunk {
                    if name == "cert" {
                        cert_data.extend_from_slice(&data);
                    } else if name == "key" {
                        key_data.extend_from_slice(&data);
                    }
                }
            }
        }
    }

    if cert_data.is_empty() || key_data.is_empty() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error("缺少证书文件或私钥文件"))
        );
    }

    // 确保证书目录存在
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    if !std::path::Path::new(&certs_dir).exists() {
        std::fs::create_dir_all(&certs_dir).map_err(|e| {
            actix_web::error::ErrorInternalServerError(format!("创建证书目录失败: {e}"))
        })?;
    }

    // 生成带时间戳的证书文件名
    let timestamp = chrono::Utc::now().timestamp();
    let base_name = format!("import_{timestamp}_cert");
    let cert_path = format!("{certs_dir}/{base_name}.pem");
    let key_path = format!("{certs_dir}/{base_name}.key");

    // 保存证书文件
    std::fs::write(&cert_path, cert_data).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("保存证书文件失败: {e}"))
    })?;

    // 保存私钥文件
    std::fs::write(&key_path, key_data).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("保存私钥文件失败: {e}"))
    })?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "证书导入成功")))
}

// 下载证书 CA
pub async fn download_certificate() -> Result<HttpResponse> {
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    // 获取当前生效的证书类型
    let cert_type = match Config::load() {
        Ok(config) => config
            .server
            .cert_type
            .unwrap_or_else(|| "self_signed".to_string()),
        Err(_) => "self_signed".to_string(),
    };

    let prefix = if cert_type == "imported" {
        "import"
    } else {
        "create"
    };

    // 查找最新的证书文件
    let mut latest_cert: Option<(String, SystemTime)> = None;

    if let Ok(entries) = std::fs::read_dir(&certs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|f| f.to_str())
                && filename.starts_with(prefix)
                && filename.ends_with(".pem")
                && let Ok(metadata) = entry.metadata()
                && let Ok(modified) = metadata.modified()
            {
                if let Some((_, latest_time)) = latest_cert {
                    if modified > latest_time {
                        latest_cert = Some((path.to_string_lossy().to_string(), modified));
                    }
                } else {
                    latest_cert = Some((path.to_string_lossy().to_string(), modified));
                }
            }
        }
    }

    if let Some((path, _)) = latest_cert {
        let content = std::fs::read(&path).map_err(actix_web::error::ErrorInternalServerError)?;
        let filename = std::path::Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("certificate.pem");

        Ok(HttpResponse::Ok()
            .content_type("application/x-pem-file")
            .append_header((
                actix_web::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ))
            .body(content))
    } else {
        Ok(HttpResponse::NotFound().body("Certificate not found"))
    }
}

// 生成自签名证书的辅助函数
fn generate_self_signed_cert(
    cert_path: &str,
    key_path: &str,
    req: &GenerateCertRequest,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    use rcgen::{
        CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, KeyPair,
        KeyUsagePurpose, SanType,
    };
    use std::convert::TryInto;

    // 1. 创建密钥对 (ECDSA P-256)
    // KeyPair::generate() 默认生成 PKCS_ECDSA_P256_SHA256
    let key_pair = KeyPair::generate()?;

    // 2. 创建证书参数
    let mut params = CertificateParams::default();

    // 设置默认有效期：从现在开始，有效期 10 年
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + time::Duration::days(3650); // 10年

    if let Some(days) = req.validity {
        params.not_after = now + time::Duration::days(i64::from(days));
    }

    // 3. 设置主体信息 (Distinguished Name)
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, &req.common_name);
    if let Some(org) = &req.organization {
        dn.push(DnType::OrganizationName, org);
    }
    if let Some(ou) = &req.organizational_unit {
        dn.push(DnType::OrganizationalUnitName, ou);
    }
    if let Some(country) = &req.country {
        dn.push(DnType::CountryName, country);
    }
    if let Some(state) = &req.state {
        dn.push(DnType::StateOrProvinceName, state);
    }
    if let Some(locality) = &req.locality {
        dn.push(DnType::LocalityName, locality);
    }
    params.distinguished_name = dn;

    // 4. 设置 SAN (Subject Alternative Name)
    let mut sans = Vec::new();

    // 从配置的 Public URL 提取 SAN
    if !config.server.public_url.is_empty() {
        let public_url = &config.server.public_url;
        // 简单解析 URL
        let host = public_url
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        let host = host.split('/').next().unwrap_or(host);
        let host = host.split(':').next().unwrap_or(host);

        if !host.is_empty() {
            if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                sans.push(SanType::IpAddress(ip));
            } else if let Ok(dns_name) = host.try_into() {
                sans.push(SanType::DnsName(dns_name));
            }
        }
    }

    // 如果没有提取到 SAN，尝试使用 Common Name
    if sans.is_empty() {
        if let Ok(ip) = req.common_name.parse::<std::net::IpAddr>() {
            sans.push(SanType::IpAddress(ip));
        } else if let Ok(dns_name) = req.common_name.as_str().try_into() {
            sans.push(SanType::DnsName(dns_name));
        }
    }
    params.subject_alt_names = sans;

    // 5. 设置 Extended Key Usage (服务器认证)
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    // 6. 设置 Key Usage (数字签名)
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];

    // 生成证书
    let cert = params.self_signed(&key_pair)?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    // 保存证书和密钥
    std::fs::write(cert_path, cert_pem)?;
    std::fs::write(key_path, key_pem)?;

    Ok(())
}

// 关闭初始化模式
pub async fn disable_init_mode(
    _pool: web::Data<DbPool>,
    config: web::Data<Config>,
) -> Result<HttpResponse> {
    tracing::info!("收到关闭初始化模式请求");

    // 创建更新后的配置，将init.enabled设置为false
    let mut new_config = config.get_ref().clone();
    new_config.init.enabled = false;

    // 将配置保存到文件
    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&new_config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to serialize config: {e}"))
    })?;

    std::fs::write(&config_path, config_str).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to write config file: {e}"))
    })?;

    tracing::info!("初始化模式已关闭，配置已保存，正在触发服务重启");

    // 使用统一的重启函数
    trigger_service_restart()
}

// 备份系统配置
pub async fn backup_config(config: web::Data<Config>) -> Result<HttpResponse> {
    // 将配置序列化为JSON格式
    let config_json = serde_json::to_string_pretty(&config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to serialize config: {e}"))
    })?;

    Ok(HttpResponse::Ok()
        .content_type("application/json")
        .append_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=ipma_config_backup_{}.json",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            ),
        ))
        .body(config_json))
}

// 恢复系统配置
pub async fn restore_config(payload: web::Json<Config>) -> Result<HttpResponse> {
    // 验证请求数据
    let new_config = payload.into_inner();

    // 验证端口配置：确保至少有一个端口是启用的
    let http_enabled = new_config.server.http_enabled.unwrap_or(false);
    let https_enabled = new_config.server.https_enabled.unwrap_or(false);

    if !http_enabled && !https_enabled {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "至少需要开启一个端口（HTTP或HTTPS）",
        )));
    }

    // 将配置保存到文件
    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&new_config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to serialize config: {e}"))
    })?;

    std::fs::write(&config_path, config_str).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to write config file: {e}"))
    })?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "系统配置恢复成功")))
}

// 语言设置更新请求模型
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateLanguageRequest {
    #[validate(length(min = 2, max = 5, message = "语言代码长度必须在2到5个字符之间"))]
    pub language: String,
}

// 会话超时配置更新请求模型
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSessionTimeoutRequest {
    pub session_timeout: Option<u64>,
}

// 页面超时配置更新请求模型
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdatePageTimeoutRequest {
    pub page_timeout: Option<u64>,
}

// 获取会话超时配置
pub async fn get_session_timeout_config(config: web::Data<Config>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "session_timeout": config.server.session_timeout
        }),
        "会话超时配置获取成功",
    )))
}

// 更新会话超时配置
pub async fn update_session_timeout_config(
    req: web::Json<UpdateSessionTimeoutRequest>,
    _config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 加载当前配置
    let mut current_config = Config::load().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to load current config: {e}"))
    })?;

    // 更新会话超时配置
    current_config.server.session_timeout = req.session_timeout;

    // 将配置保存到文件
    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to serialize config: {e}"))
    })?;

    std::fs::write(&config_path, config_str).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to write config file: {e}"))
    })?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "会话超时配置更新成功")))
}

// 获取支持的语言列表
pub async fn get_supported_languages() -> Result<HttpResponse> {
    // 定义支持的语言列表
    let supported_languages = vec![
        serde_json::json!({
            "code": "en",
            "name": "English",
            "native_name": "English"
        }),
        serde_json::json!({
            "code": "zh",
            "name": "Chinese",
            "native_name": "中文"
        }),
    ];

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        supported_languages,
        "获取支持的语言列表成功",
    )))
}

// 更新用户语言设置
pub async fn update_language_setting(
    _pool: web::Data<DbPool>,
    req: web::Json<UpdateLanguageRequest>,
    _config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 验证请求数据
    if let Err(e) = (*req).validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("验证错误: {e:?}")))
        );
    }

    // 检查语言代码是否有效
    let language = req.language.to_lowercase();
    if language != "en" && language != "zh" {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(
            "不支持的语言代码，请使用 'en' 或 'zh'",
        )));
    }

    // 加载当前配置
    let mut current_config = Config::load().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to load current config: {e}"))
    })?;

    // 更新语言设置
    current_config.i18n = Some(I18nConfig {
        default_language: language,
        supported_languages: vec!["zh".to_string(), "en".to_string()],
    });

    // 将配置保存到文件
    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to serialize config: {e}"))
    })?;

    std::fs::write(&config_path, config_str).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to write config file: {e}"))
    })?;

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "语言设置更新成功")))
}

// 获取页面超时配置
pub async fn get_page_timeout_config(config: web::Data<Config>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "page_timeout": config.server.page_timeout
        }),
        "页面超时配置获取成功",
    )))
}

// 更新页面超时配置
pub async fn update_page_timeout_config(
    req: web::Json<UpdatePageTimeoutRequest>,
    _config: web::Data<Config>,
) -> Result<HttpResponse> {
    // 加载当前配置
    let mut current_config = Config::load().map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to load current config: {e}"))
    })?;

    // 更新页面超时配置
    current_config.server.page_timeout = req.page_timeout;

    // 将配置保存到文件
    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to serialize config: {e}"))
    })?;

    std::fs::write(&config_path, config_str).map_err(|e| {
        actix_web::error::ErrorInternalServerError(format!("Failed to write config file: {e}"))
    })?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "页面超时配置更新成功")))
}

// 通知设置请求模型
#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub email_recipients: Vec<Uuid>,
}

// 获取通知设置
pub async fn get_notification_settings(pool: web::Data<DbPool>) -> Result<HttpResponse> {
    let recipients = match sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = 'notification' AND key = 'email_recipients'",
    )
    .fetch_optional(&pool.get_conn())
    .await
    {
        Ok(Some(recips)) => {
            recips.split(',')
                .filter_map(|id| Uuid::parse_str(id.trim()).ok())
                .collect::<Vec<Uuid>>()
        }
        Ok(None) | Err(_) => Vec::new(),
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        NotificationSettings {
            email_recipients: recipients,
        },
        "通知设置获取成功",
    )))
}

// 更新通知设置
pub async fn update_notification_settings(
    pool: web::Data<DbPool>,
    req: web::Json<NotificationSettings>,
) -> Result<HttpResponse> {
    let recipients_str = req
        .email_recipients
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<String>>()
        .join(",");

    sqlx::query(
        "INSERT INTO system_configs (config_type, key, value) 
         VALUES ('notification', 'email_recipients', $1) 
         ON CONFLICT (config_type, key) 
         DO UPDATE SET value = $1, updated_at = NOW()",
    )
    .bind(&recipients_str)
    .execute(&pool.get_conn())
    .await
    .map_err(|e| actix_web::error::ErrorInternalServerError(format!("保存通知设置失败: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "通知设置更新成功")))
}
