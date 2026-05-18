use crate::app_state::AppState;
use crate::config::{Config, I18nConfig, ServerConfig};
use crate::error::AppError;
use crate::models::ApiResponse;
use crate::system::smtp::SmtpConfig;
use crate::system::smtp::{
    get_smtp_config_from_db, save_smtp_config_to_db, send_email_to_users,
};
use actix_web::{HttpResponse, web};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use uuid::Uuid;
use validator::Validate;

static START_TIME: AtomicU64 = AtomicU64::new(0);

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

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSystemConfigRequest {
    pub database: Option<crate::config::DatabaseConfig>,
    pub server: Option<ServerConfig>,
    pub jwt: Option<crate::config::JwtConfig>,
    pub init: Option<crate::config::InitConfig>,
    pub rate_limit: Option<crate::config::RateLimitConfig>,
    pub snmp: Option<crate::config::SnmpConfig>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct GenerateCertRequest {
    #[validate(length(min = 1, message = "通用名称不能为空"))]
    pub common_name: String,
    pub organization: Option<String>,
    pub organizational_unit: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub locality: Option<String>,
    pub validity: Option<i32>,
    pub subject_alt_names: Option<Vec<String>>,
}

pub fn record_start_time() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    START_TIME.store(now, Ordering::SeqCst);
}

pub fn init_start_time() {
    record_start_time();
}

async fn save_config_to_file(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = crate::config::get_config_file_path();
    tracing::info!("[save_config] 开始保存配置到: {}", config_path);

    let toml_str = toml::to_string_pretty(config)?;
    tracing::info!("[save_config] TOML 内容长度: {}", toml_str.len());

    tokio::fs::write(&config_path, &toml_str).await?;
    tracing::info!("[save_config] 配置已写入文件");

    let verify_content = tokio::fs::read_to_string(&config_path).await?;
    tracing::info!(
        "[save_config] 验证读取成功，内容长度: {}",
        verify_content.len()
    );

    Ok(())
}

pub async fn get_system_info(
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let database_status = match sqlx::query("SELECT 1").execute(&state.pool()?.get_conn()).await {
        Ok(_) => "connected".to_string(),
        Err(e) => {
            tracing::error!("数据库连接检查失败: {}", e);
            format!("disconnected: {}", e)
        }
    };

    let uptime = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        - START_TIME.load(Ordering::SeqCst);

    let pool_metrics = state.pool()?.get_metrics();

    let system_info = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "database_status": database_status,
        "uptime_seconds": uptime,
        "pool_metrics": {
            "active_connections": pool_metrics.active_connections,
            "idle_connections": pool_metrics.idle_connections,
            "waiting_requests": pool_metrics.waiting_requests,
        }
    });

    Ok(HttpResponse::Ok().json(ApiResponse::success(system_info, "系统信息获取成功")))
}

pub async fn get_system_config(
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        state.config.clone(),
        "系统配置获取成功",
    )))
}

pub async fn update_system_config(
    state: web::Data<AppState>,
    req: web::Json<UpdateSystemConfigRequest>,
) -> Result<HttpResponse, AppError> {
    let mut new_config = state.config.clone();

    if let Some(database) = &req.database {
        new_config.database = database.clone();
    }

    if let Some(server) = &req.server {
        let host = server.host.trim();
        let host_ipv6 = server.host_ipv6.as_ref()
            .map(|s| s.trim())
            .unwrap_or("");

        fn validate_ip(addr: &str) -> Result<(), String> {
            if addr.is_empty() {
                return Ok(());
            }
            if addr.parse::<std::net::IpAddr>().is_ok() {
                Ok(())
            } else {
                Err(format!("无效的IP地址格式: {}", addr))
            }
        }

        validate_ip(host).map_err(AppError::Validation)?;
        validate_ip(host_ipv6).map_err(AppError::Validation)?;

        if host.is_empty() && host_ipv6.is_empty() {
            return Err(AppError::Validation("至少需要配置一个监听地址（IPv4或IPv6）".to_string()));
        }

        new_config.server = server.clone();
    }

    if let Some(jwt) = &req.jwt {
        new_config.jwt = jwt.clone();
    }

    if let Some(init) = &req.init {
        new_config.init = init.clone();
    }

    if let Some(rate_limit) = &req.rate_limit {
        new_config.rate_limit = rate_limit.clone();
    }

    if let Some(snmp) = &req.snmp {
        new_config.snmp = snmp.clone();
    }

    let config_path = crate::config::get_config_file_path();
    tracing::info!("[update_config] 准备保存配置到: {}", config_path);

    save_config_to_file(&new_config).await.map_err(|e| AppError::Internal(format!("配置保存失败: {e:?}")))?;
    tracing::info!("配置已保存到: {}", config_path);

    Ok(HttpResponse::Ok().json(ApiResponse::success(new_config, "配置更新成功")))
}

pub async fn trigger_service_restart() -> Result<HttpResponse, AppError> {
    let service_name = "ipma.service";

    let is_running_as_service = tokio::task::spawn_blocking(check_if_running_as_service)
        .await
        .unwrap_or(false);

    tracing::info!(
        "触发服务重启, is_running_as_service: {}",
        is_running_as_service
    );

    if is_running_as_service {
        let check_output = Command::new("systemctl")
            .args(["show", "ipma.service", "--property=ActiveState"])
            .output()
            .await;

        let is_active = match check_output {
            Ok(output) => {
                let status = String::from_utf8_lossy(&output.stdout);
                status.contains("ActiveState=active")
            }
            Err(_) => true,
        };

        tracing::info!("服务当前状态: active={}", is_active);

        let output = Command::new("systemctl")
            .arg("restart")
            .arg(service_name)
            .output()
            .await;

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

                if output.status.success() {
                    tracing::info!("systemctl restart 执行成功");
                    return Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
                        (),
                        "服务重启命令已发送，服务正在重启...",
                    )));
                }

                if stderr.contains("Access denied")
                    || stderr.contains("Permission denied")
                    || stderr.contains("Interactive authentication required")
                {
                    tracing::info!("权限不足，使用进程退出方式重启");
                    return restart_by_exit();
                }

                if stderr.contains("Failed") && !stderr.contains("Failed to restart") {
                    tracing::error!("服务重启失败: {}", stderr);
                    return Err(AppError::Internal(format!("服务重启失败: {}", stderr.trim())));
                }

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
        restart_standalone_process().await
    }
}

pub async fn restart_application() -> Result<HttpResponse, AppError> {
    tracing::info!("收到重启应用请求");
    trigger_service_restart().await
}

fn restart_by_exit() -> Result<HttpResponse, AppError> {
    tracing::info!("使用进程退出方式触发重启（systemd Restart=always 会自动重启）");

    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
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

async fn restart_standalone_process() -> Result<HttpResponse, AppError> {
    let exe_path = std::env::current_exe().map_err(|e| AppError::Internal(format!("获取可执行文件路径失败: {e}")))?;

    let exe_path_str = exe_path.to_str().ok_or_else(|| AppError::Internal("无法将可执行文件路径转换为字符串".to_string()))?;

    let working_dir = std::env::current_dir().map_err(|e| AppError::Internal(format!("获取工作目录失败: {e}")))?;

    let working_dir_str = working_dir.to_str().ok_or_else(|| AppError::Internal("无法将工作目录路径转换为字符串".to_string()))?;

    let restart_script = format!(
        r#"#!/bin/bash
sleep 3
cd "{working_dir_str}"
exec "{exe_path_str}"
"#
    );

    let script_path = "/tmp/ipma_restart.sh";
    tokio::fs::write(script_path, restart_script).await.map_err(|e| AppError::Internal(format!("创建重启脚本失败: {e}")))?;

    let output = Command::new("chmod")
        .arg("+x")
        .arg(script_path)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("设置脚本权限失败: {e}")))?;

    if !output.status.success() {
        return Err(AppError::Internal("设置脚本权限失败".to_string()));
    }

    if let Err(e) = Command::new("nohup").arg(script_path).spawn() {
        tracing::warn!("启动重启脚本失败: {}", e);
    }

    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        std::process::exit(0);
    });

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "服务重启命令已发送，服务正在重启",
    )))
}

pub async fn restart_os() -> Result<HttpResponse, AppError> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("sleep 2 && sudo reboot")
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("执行重启命令失败: {e}")))?;

    if !output.status.success() {
        let error_message = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!(
            "操作系统重启失败: {error_message}"
        )));
    }

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success(
        (),
        "操作系统重启命令已发送，系统将在2秒后重启",
    )))
}

#[derive(Debug, Serialize)]
pub struct CertificateStatus {
    pub has_imported_cert: bool,
    pub has_self_signed_cert: bool,
    pub cert_type: String,
}

async fn check_certificate_exists(dir: &str, cert_type: &str) -> bool {
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
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

pub async fn get_certificate_status() -> Result<HttpResponse, AppError> {
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    let has_imported_cert = check_certificate_exists(&certs_dir, "import").await;

    let has_self_signed_cert = check_certificate_exists(&certs_dir, "create").await;

    let cert_type = match tokio::task::spawn_blocking(Config::load)
        .await
        .unwrap_or(Err(config::ConfigError::Message("spawn_blocking failed".into())))
    {
        Ok(config) => config
            .server
            .cert_type
            .unwrap_or_else(|| "self_signed".to_string()),
        Err(_) => "self_signed".to_string(),
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        CertificateStatus {
            has_imported_cert,
            has_self_signed_cert,
            cert_type,
        },
        "证书状态获取成功",
    )))
}

pub async fn generate_certificate(
    state: web::Data<AppState>,
    req: web::Json<GenerateCertRequest>,
) -> Result<HttpResponse, AppError> {
    req.validate()?;

    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    if !tokio::fs::try_exists(&certs_dir).await.unwrap_or(false) {
        tokio::fs::create_dir_all(&certs_dir).await.map_err(|e| AppError::Internal(format!("创建证书目录失败: {e}")))?;
    }

    let timestamp = chrono::Utc::now().timestamp();
    let base_name = format!("create_{timestamp}_cert");
    let cert_path = format!("{certs_dir}/{base_name}.pem");
    let key_path = format!("{certs_dir}/{base_name}.key");

    let cert_path_clone = cert_path.clone();
    let key_path_clone = key_path.clone();
    let req_clone = req.into_inner();
    let config = state.config.clone();
    tokio::task::spawn_blocking(move || {
        generate_self_signed_cert(&cert_path_clone, &key_path_clone, &req_clone, &config)
    })
    .await
    .map_err(|e| AppError::Internal(format!("生成证书任务失败: {e}")))?
    .map_err(|e| AppError::Internal(format!("生成证书失败: {e:?}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "证书生成成功")))
}

pub async fn import_certificate(
    _state: web::Data<AppState>,
    req: actix_multipart::Multipart,
) -> Result<HttpResponse, AppError> {
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
        return Err(AppError::Validation("缺少证书文件或私钥文件".to_string()));
    }

    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    if !tokio::fs::try_exists(&certs_dir).await.unwrap_or(false) {
        tokio::fs::create_dir_all(&certs_dir).await.map_err(|e| AppError::Internal(format!("创建证书目录失败: {e}")))?;
    }

    let timestamp = chrono::Utc::now().timestamp();
    let base_name = format!("import_{timestamp}_cert");
    let cert_path = format!("{certs_dir}/{base_name}.pem");
    let key_path = format!("{certs_dir}/{base_name}.key");

    tokio::fs::write(&cert_path, cert_data).await.map_err(|e| AppError::Internal(format!("保存证书文件失败: {e}")))?;

    tokio::fs::write(&key_path, key_data).await.map_err(|e| AppError::Internal(format!("保存私钥文件失败: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "证书导入成功")))
}

pub async fn download_certificate() -> Result<HttpResponse, AppError> {
    let app_name = env!("CARGO_PKG_NAME");
    let certs_dir = format!("/etc/{app_name}/certs");

    let cert_type = match tokio::task::spawn_blocking(Config::load)
        .await
        .unwrap_or(Err(config::ConfigError::Message("spawn_blocking failed".into())))
    {
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

    let mut latest_cert: Option<(String, SystemTime)> = None;

    if let Ok(mut entries) = tokio::fs::read_dir(&certs_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|f| f.to_str())
                && filename.starts_with(prefix)
                && filename.ends_with(".pem")
                && let Ok(metadata) = entry.metadata().await
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
        let content = tokio::fs::read(&path).await.map_err(|e| AppError::Internal(e.to_string()))?;
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
        Err(AppError::Validation("未找到证书文件".to_string()))
    }
}

fn generate_self_signed_cert(
    cert_path: &str,
    key_path: &str,
    req: &GenerateCertRequest,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rcgen::{
        CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, KeyPair,
        KeyUsagePurpose, SanType,
    };
    use std::convert::TryInto;

    let key_pair = KeyPair::generate()?;

    let mut params = CertificateParams::default();

    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + time::Duration::days(3650);

    if let Some(days) = req.validity {
        params.not_after = now + time::Duration::days(i64::from(days));
    }

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

    let mut sans = Vec::new();

    if !config.server.public_url.is_empty() {
        let public_url = &config.server.public_url;
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

    if sans.is_empty() {
        if let Ok(ip) = req.common_name.parse::<std::net::IpAddr>() {
            sans.push(SanType::IpAddress(ip));
        } else if let Ok(dns_name) = req.common_name.as_str().try_into() {
            sans.push(SanType::DnsName(dns_name));
        }
    }
    params.subject_alt_names = sans;

    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];

    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];

    let cert = params.self_signed(&key_pair)?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    std::fs::write(cert_path, cert_pem)?;
    std::fs::write(key_path, key_pem)?;

    Ok(())
}

pub async fn disable_init_mode(
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    tracing::info!("收到关闭初始化模式请求");

    let mut new_config = state.config.clone();
    new_config.init.enabled = false;

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&new_config).map_err(|e| AppError::Internal(format!("Failed to serialize config: {e}")))?;

    tokio::fs::write(&config_path, config_str).await.map_err(|e| AppError::Internal(format!("Failed to write config file: {e}")))?;

    tracing::info!("初始化模式已关闭，配置已保存，正在触发服务重启");

    trigger_service_restart().await
}

pub async fn backup_config(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let config_json = serde_json::to_string_pretty(&state.config).map_err(|e| AppError::Internal(format!("Failed to serialize config: {e}")))?;

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

pub async fn restore_config(payload: web::Json<Config>) -> Result<HttpResponse, AppError> {
    let new_config = payload.into_inner();

    let http_enabled = new_config.server.http_enabled.unwrap_or(false);
    let https_enabled = new_config.server.https_enabled.unwrap_or(false);

    if !http_enabled && !https_enabled {
        return Err(AppError::Validation("至少需要开启一个端口（HTTP或HTTPS）".to_string()));
    }

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&new_config).map_err(|e| AppError::Internal(format!("Failed to serialize config: {e}")))?;

    tokio::fs::write(&config_path, config_str).await.map_err(|e| AppError::Internal(format!("Failed to write config file: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "系统配置恢复成功")))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateLanguageRequest {
    #[validate(length(min = 2, max = 5, message = "语言代码长度必须在2到5个字符之间"))]
    pub language: String,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSessionTimeoutRequest {
    pub session_timeout: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdatePageTimeoutRequest {
    pub page_timeout: Option<u64>,
}

pub async fn get_session_timeout_config(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "session_timeout": state.config.server.session_timeout
        }),
        "会话超时配置获取成功",
    )))
}

pub async fn update_session_timeout_config(
    req: web::Json<UpdateSessionTimeoutRequest>,
    _state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let mut current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| AppError::Internal(format!("配置加载任务失败: {e}")))?
        .map_err(|e| AppError::Internal(format!("Failed to load current config: {e}")))?;

    current_config.server.session_timeout = req.session_timeout;

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| AppError::Internal(format!("Failed to serialize config: {e}")))?;

    tokio::fs::write(&config_path, config_str).await.map_err(|e| AppError::Internal(format!("Failed to write config file: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "会话超时配置更新成功")))
}

pub async fn get_supported_languages() -> Result<HttpResponse, AppError> {
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

pub async fn update_language_setting(
    _state: web::Data<AppState>,
    req: web::Json<UpdateLanguageRequest>,
) -> Result<HttpResponse, AppError> {
    req.validate()?;

    let language = req.language.to_lowercase();
    if language != "en" && language != "zh" {
        return Err(AppError::Validation("不支持的语言代码，请使用 'en' 或 'zh'".to_string()));
    }

    let mut current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| AppError::Internal(format!("配置加载任务失败: {e}")))?
        .map_err(|e| AppError::Internal(format!("Failed to load current config: {e}")))?;

    current_config.i18n = Some(I18nConfig {
        default_language: language,
        supported_languages: vec!["zh".to_string(), "en".to_string()],
    });

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| AppError::Internal(format!("Failed to serialize config: {e}")))?;

    tokio::fs::write(&config_path, config_str).await.map_err(|e| AppError::Internal(format!("Failed to write config file: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::success((), "语言设置更新成功")))
}

pub async fn get_page_timeout_config(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        serde_json::json!({
            "page_timeout": state.config.server.page_timeout
        }),
        "页面超时配置获取成功",
    )))
}

pub async fn update_page_timeout_config(
    req: web::Json<UpdatePageTimeoutRequest>,
    _state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let mut current_config = tokio::task::spawn_blocking(Config::load)
        .await
        .map_err(|e| AppError::Internal(format!("配置加载任务失败: {e}")))?
        .map_err(|e| AppError::Internal(format!("Failed to load current config: {e}")))?;

    current_config.server.page_timeout = req.page_timeout;

    let config_path = crate::config::get_config_file_path();
    let config_str = toml::to_string(&current_config).map_err(|e| AppError::Internal(format!("Failed to serialize config: {e}")))?;

    tokio::fs::write(&config_path, config_str).await.map_err(|e| AppError::Internal(format!("Failed to write config file: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "页面超时配置更新成功")))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub email_recipients: Vec<Uuid>,
}

pub async fn get_notification_settings(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let recipients = match sqlx::query_scalar::<_, String>(
        "SELECT value FROM system_configs WHERE config_type = 'notification' AND key = 'email_recipients'",
    )
    .fetch_optional(&state.pool()?.get_conn())
    .await
    {
        Ok(Some(value)) => {
            serde_json::from_str(&value).unwrap_or_default()
        }
        _ => Vec::new(),
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        NotificationSettings { email_recipients: recipients },
        "通知设置获取成功",
    )))
}

pub async fn update_notification_settings(
    state: web::Data<AppState>,
    req: web::Json<NotificationSettings>,
) -> Result<HttpResponse, AppError> {
    let value = serde_json::to_string(&req.email_recipients)
        .map_err(|e| AppError::Internal(format!("序列化失败: {e}")))?;

    sqlx::query(
        "INSERT INTO system_configs (config_type, key, value) VALUES ('notification', 'email_recipients', $1)
         ON CONFLICT (config_type, key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(&value)
    .execute(&state.pool()?.get_conn())
    .await
    .map_err(|e| AppError::Internal(format!("数据库操作失败: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "通知设置更新成功")))
}

pub async fn get_smtp_config(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    let config = match get_smtp_config_from_db(&state.pool()?.get_conn()).await {
        Some(c) => c,
        None => return Err(AppError::Internal("SMTP配置未设置".to_string())),
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(config, "SMTP配置获取成功")))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateSmtpConfigRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub secure: bool,
}

pub async fn update_smtp_config(
    state: web::Data<AppState>,
    req: web::Json<UpdateSmtpConfigRequest>,
) -> Result<HttpResponse, AppError> {
    let config = SmtpConfig {
        host: req.host.clone(),
        port: req.port,
        username: req.username.clone(),
        password: req.password.clone(),
        from: req.from.clone(),
        secure: req.secure,
    };

    save_smtp_config_to_db(&state.pool()?.get_conn(), &config).await
        .map_err(|e| AppError::Internal(format!("保存SMTP配置失败: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "SMTP配置更新成功")))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct TestSmtpRequest {
    #[validate(email(message = "邮箱格式不正确"))]
    pub to: String,
}

pub async fn test_smtp_connection(
    state: web::Data<AppState>,
    req: web::Json<TestSmtpRequest>,
) -> Result<HttpResponse, AppError> {
    req.validate()?;

    let config = match get_smtp_config_from_db(&state.pool()?.get_conn()).await {
        Some(c) => c,
        None => return Err(AppError::Internal("SMTP配置未设置".to_string())),
    };

    crate::system::smtp::test_smtp_connection(&config).await
        .map_err(|e| AppError::Internal(format!("SMTP测试失败: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "SMTP连接测试成功")))
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct SendSystemEmailRequest {
    #[validate(length(min = 1, message = "收件人不能为空"))]
    pub user_ids: Vec<Uuid>,
    #[validate(length(min = 1, max = 255, message = "主题长度必须在1到255个字符之间"))]
    pub subject: String,
    #[validate(length(min = 1, message = "邮件内容不能为空"))]
    pub body: String,
}

pub async fn send_system_email(
    state: web::Data<AppState>,
    req: web::Json<SendSystemEmailRequest>,
) -> Result<HttpResponse, AppError> {
    req.validate()?;

    send_email_to_users(
        &state.pool()?.get_conn(),
        &req.user_ids,
        &req.subject,
        &req.body,
    )
    .await
    .map_err(|e| AppError::Internal(format!("发送邮件失败: {e}")))?;

    Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "邮件发送成功")))
}

#[derive(Debug, Serialize)]
pub struct ServiceStatus {
    pub registered: bool,
    pub running_as_service: bool,
    pub service_file_exists: bool,
    pub active: bool,
    pub status: Option<String>,
    pub enabled: bool,
    pub uptime_seconds: Option<u64>,
}

pub async fn get_service_status() -> Result<HttpResponse, AppError> {
    let running_as_service = check_if_running_as_service();
    let service_file_exists = std::path::Path::new("/etc/systemd/system/ipma.service").exists();

    let (active, status, enabled, uptime_seconds) = if running_as_service {
        let active_output = Command::new("systemctl")
            .args(["show", "ipma.service", "--property=ActiveState"])
            .output()
            .await;

        let active = match active_output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.contains("ActiveState=active")
            }
            Err(_) => false,
        };

        let status_output = Command::new("systemctl")
            .args(["show", "ipma.service", "--property=StatusText"])
            .output()
            .await;

        let status = match status_output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let status_text = stdout.trim().strip_prefix("StatusText=");
                status_text.map(|s| s.to_string())
            }
            Err(_) => None,
        };

        let enabled_output = Command::new("systemctl")
            .args(["is-enabled", "ipma.service"])
            .output()
            .await;

        let enabled = match enabled_output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.trim() == "enabled"
            }
            Err(_) => false,
        };

        let uptime_seconds = if active {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let start_time = START_TIME.load(Ordering::SeqCst);
            Some(now.saturating_sub(start_time))
        } else {
            None
        };

        (active, status, enabled, uptime_seconds)
    } else {
        (false, None, false, None)
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        ServiceStatus {
            registered: service_file_exists,
            running_as_service,
            service_file_exists,
            active,
            status,
            enabled,
            uptime_seconds,
        },
        "服务状态获取成功",
    )))
}

pub async fn register_service() -> Result<HttpResponse, AppError> {
    let exe_path = std::env::current_exe()
        .map_err(|e| AppError::Internal(format!("获取可执行文件路径失败: {e}")))?;
    let exe_path_str = exe_path.to_str()
        .ok_or_else(|| AppError::Internal("无法将可执行文件路径转换为字符串".to_string()))?;

    let working_dir = std::env::current_dir()
        .map_err(|e| AppError::Internal(format!("获取工作目录失败: {e}")))?;
    let working_dir_str = working_dir.to_str()
        .ok_or_else(|| AppError::Internal("无法将工作目录路径转换为字符串".to_string()))?;

    let service_content = format!(
        r#"[Unit]
Description=IPMA - IP/MAC Address Management System
After=network.target postgresql.service

[Service]
Type=simple
WorkingDirectory={}
ExecStart={}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"#,
        working_dir_str, exe_path_str
    );

    let service_path = "/etc/systemd/system/ipma.service";
    tokio::fs::write(service_path, service_content)
        .await
        .map_err(|e| AppError::Internal(format!("写入服务文件失败: {e}")))?;

    let daemon_reload = Command::new("systemctl")
        .arg("daemon-reload")
        .output()
        .await;

    if let Err(e) = daemon_reload {
        tracing::warn!("daemon-reload 执行失败: {}", e);
    }

    let enable_output = Command::new("systemctl")
        .args(["enable", "ipma.service"])
        .output()
        .await;

    if let Err(e) = enable_output {
        tracing::warn!("enable 服务失败: {}", e);
    }

    let start_output = Command::new("systemctl")
        .args(["start", "ipma.service"])
        .output()
        .await;

    match start_output {
        Ok(output) if output.status.success() => {
            Ok(HttpResponse::Ok().json(ApiResponse::<()>::success((), "服务注册并启动成功")))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(AppError::Internal(format!("启动服务失败: {}", stderr.trim())))
        }
        Err(e) => Err(AppError::Internal(format!("启动服务失败: {e}"))),
    }
}

pub async fn get_dashboard_stats(
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let networks_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM network_cidrs")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let regions_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM network_regions")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let ips_total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ips")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let ips_active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ips WHERE status = 'active'")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let rooms_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let cabinets_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cabinets")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let workstations_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workstations")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let switches_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM switches")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let users_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool()?.get_conn())
        .await?;

    let stats = serde_json::json!({
        "networks": {
            "networks": networks_count,
            "regions": regions_count
        },
        "ips": {
            "total": ips_total,
            "active": ips_active,
            "inactive": ips_total - ips_active
        },
        "resources": {
            "rooms": rooms_count,
            "cabinets": cabinets_count,
            "workstations": workstations_count,
            "switches": switches_count
        },
        "users": {
            "total": users_count
        }
    });

    Ok(HttpResponse::Ok().json(ApiResponse::success(stats, "仪表盘统计获取成功")))
}
