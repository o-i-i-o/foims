//! SNMP 通用采集（端口/MAC/LLDP 参数构造与请求）。

use std::sync::Arc;
use std::time::Duration;

use async_snmp::{
    Auth, Client, Error, oid,
    v3::{AuthProtocol, PrivProtocol},
};
use axum::extract::{Path, State};
use axum::response::Response;
use thiserror::Error;
use tracing::debug;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::crypto::decrypt_credential_async;
use crate::error::{AppError, msg};
use crate::models::{DevicePortCreate, SnmpTestRequest};
use crate::routes::static_files::AppJson;
use ipma_common::{AppMessage, log_info, log_warn};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeviceForSnmp {
    pub id: Uuid,
    pub name: String,
    pub snmp_version: String,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: i32,
}

impl DeviceForSnmp {
    pub async fn to_snmp_params_async(
        &self,
        ip_address: &str,
    ) -> Result<SnmpParamsLegacy, AppError> {
        let creds = DecryptedSnmpCredentials::from_device_snmp_async(self).await?;
        Ok(SnmpParamsLegacy {
            ip: ip_address.to_string(),
            port: self.snmp_port,
            version: self.snmp_version.clone(),
            community: creds.community,
            username: self.snmp_username.clone(),
            auth_proto: self.snmp_auth_protocol.clone(),
            auth_pass: creds.auth_password,
            priv_proto: self.snmp_priv_protocol.clone(),
            priv_pass: creds.priv_password,
            timeout_secs: 10,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DecryptedSnmpCredentials {
    pub community: Option<String>,
    pub auth_password: Option<String>,
    pub priv_password: Option<String>,
}

impl DecryptedSnmpCredentials {
    pub async fn from_device_snmp_async(switch: &DeviceForSnmp) -> Result<Self, AppError> {
        let community = decrypt_credential_async(switch.snmp_community.clone()).await?;
        let auth_password = decrypt_credential_async(switch.snmp_auth_password.clone()).await?;
        let priv_password = decrypt_credential_async(switch.snmp_priv_password.clone()).await?;
        Ok(Self {
            community,
            auth_password,
            priv_password,
        })
    }
}

#[derive(Error, Debug)]
pub enum SnmpError {
    #[error("{0}")]
    Message(String),
}

impl From<AppError> for SnmpError {
    fn from(e: AppError) -> Self {
        SnmpError::Message(e.to_string())
    }
}

#[derive(Error, Debug)]
pub enum SwitchConfigError {
    #[error("{}", .0.key())]
    NotFound(AppMessage),
    #[error("{}", .0.key())]
    Database(AppMessage),
}

impl From<SwitchConfigError> for AppError {
    fn from(e: SwitchConfigError) -> Self {
        match e {
            SwitchConfigError::NotFound(m) => AppError::NotFound(m),
            SwitchConfigError::Database(m) => AppError::Database(m),
        }
    }
}

impl From<SwitchConfigError> for SnmpError {
    fn from(e: SwitchConfigError) -> Self {
        match e {
            SwitchConfigError::NotFound(m) | SwitchConfigError::Database(m) => {
                SnmpError::Message(m.log_string())
            }
        }
    }
}

pub async fn get_device_snmp_config(
    pool: &sqlx::PgPool,
    device_id: &Uuid,
) -> Result<(DeviceForSnmp, Option<String>), SwitchConfigError> {
    let switch = sqlx::query_as::<_, DeviceForSnmp>(
        r"SELECT
            id, name, snmp_version, snmp_community,
            snmp_username, snmp_auth_protocol,
            snmp_auth_password, snmp_priv_protocol,
            snmp_priv_password, snmp_port
        FROM devices WHERE id = $1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchConfigError::Database(msg("server.error.database").with("error", e)))?
    .ok_or_else(|| SwitchConfigError::NotFound(msg("server.device.not_found")))?;

    let ip_address = get_device_ip_address(pool, device_id).await?;

    Ok((switch, ip_address))
}

pub async fn get_device_ip_address(
    pool: &sqlx::PgPool,
    device_id: &Uuid,
) -> Result<Option<String>, SwitchConfigError> {
    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(i.ip_address) FROM ips i
           JOIN device_interfaces di ON i.device_interface_id = di.id
           WHERE di.device_id = $1
           ORDER BY i.created_at LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| SwitchConfigError::Database(msg("server.error.database").with("error", e)))?
    .flatten();

    Ok(ip_address.filter(|ip| !ip.is_empty()))
}

#[derive(Debug, Clone)]
pub struct SnmpParamsLegacy {
    pub ip: String,
    pub port: i32,
    pub version: String,
    pub community: Option<String>,
    pub username: Option<String>,
    pub auth_proto: Option<String>,
    pub auth_pass: Option<String>,
    pub priv_proto: Option<String>,
    pub priv_pass: Option<String>,
    pub timeout_secs: u64,
}

pub fn build_auth(params: &SnmpParamsLegacy) -> Result<Auth, String> {
    match params.version.as_str() {
        "v1" | "v2c" => {
            let community = params
                .community
                .as_deref()
                .ok_or_else(|| "SNMP v1/v2c 需要配置 community 字符串".to_string())?;
            Ok(Auth::v2c(community))
        }
        "v3" => {
            let username = params
                .username
                .as_deref()
                .ok_or_else(|| "SNMPv3需要用户名".to_string())?;
            let usm = Auth::usm(username);

            let usm = match (params.auth_proto.as_deref(), params.auth_pass.as_deref()) {
                (Some(proto), Some(auth_pass)) => {
                    let auth_protocol = match proto {
                        "MD5" => AuthProtocol::Md5,
                        "SHA" | "SHA-1" | "SHA1" => AuthProtocol::Sha1,
                        "SHA-224" => AuthProtocol::Sha224,
                        "SHA-256" => AuthProtocol::Sha256,
                        "SHA-384" => AuthProtocol::Sha384,
                        "SHA-512" => AuthProtocol::Sha512,
                        _ => return Err(format!("不支持的认证协议: {proto}")),
                    };

                    match (params.priv_proto.as_deref(), params.priv_pass.as_deref()) {
                        (Some(proto), Some(priv_pass)) => {
                            let priv_protocol = match proto {
                                "DES" => PrivProtocol::Des,
                                "3DES" | "DES3" => PrivProtocol::Des3,
                                "AES" | "AES-128" | "AES128" => PrivProtocol::Aes128,
                                "AES-192" | "AES192" => PrivProtocol::Aes192,
                                "AES-256" | "AES256" => PrivProtocol::Aes256,
                                _ => return Err(format!("不支持的隐私协议: {proto}")),
                            };
                            usm.auth_priv(auth_protocol, auth_pass, priv_protocol, priv_pass)
                        }
                        _ => usm.auth(auth_protocol, auth_pass),
                    }
                }
                _ => usm,
            };

            Ok(usm.into())
        }
        _ => Err(format!("不支持的SNMP版本: {}", params.version)),
    }
}

#[must_use]
pub fn format_snmp_error(e: Box<Error>) -> String {
    match *e {
        Error::Timeout {
            target, retries, ..
        } => {
            format!("连接超时 (目标: {target}, 重试次数: {retries})")
        }
        Error::Network { target, source } => {
            format!("网络错误 (目标: {target}): {source}")
        }
        Error::Snmp {
            target,
            status,
            index,
            ..
        } => {
            format!("SNMP错误 (目标: {target}, 状态: {status:?}, 索引: {index})")
        }
        Error::Auth { target } => {
            format!("认证失败 (目标: {target})")
        }
        Error::MalformedResponse { target } => {
            format!("响应格式错误 (目标: {target})")
        }
        Error::WalkAborted { target, reason } => {
            format!("Walk中断 (目标: {target}, 原因: {reason:?})")
        }
        Error::Config(msg) => {
            format!("配置错误: {msg}")
        }
        Error::InvalidOid(oid) => {
            format!("无效OID: {oid}")
        }
        _ => format!("未知错误: {e:?}"),
    }
}

pub async fn test_snmp(params: &SnmpParamsLegacy, timeout_secs: u64) -> Result<String, SnmpError> {
    let addr = format!("{}:{}", params.ip, params.port);
    let timeout = Duration::from_secs(timeout_secs);

    log_info!(
        "log.device.snmp.test_connecting",
        addr = addr,
        timeout = timeout_secs
    );

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    log_info!("log.device.snmp.auth_built");

    debug!("连接SNMP设备: {} (版本: {})", addr, params.version);

    let client = Client::builder(&addr, auth)
        .timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let result = client
        .get(&oid!(1, 3, 6, 1, 2, 1, 1, 1, 0))
        .await
        .map_err(|e| SnmpError::Message(format!("SNMP请求失败: {}", format_snmp_error(e))))?;

    if result.value.is_exception() {
        match &result.value {
            async_snmp::Value::NoSuchObject => {
                debug!("SNMP响应: NoSuchObject (OID存在但无值)");
                Ok(format!(
                    "SNMP连接成功 ({}:{})，但设备不支持sysDescr OID",
                    params.ip, params.port
                ))
            }
            async_snmp::Value::NoSuchInstance => {
                debug!("SNMP响应: NoSuchInstance (实例不存在)");
                Ok(format!(
                    "SNMP连接成功 ({}:{})，但sysDescr实例不存在",
                    params.ip, params.port
                ))
            }
            async_snmp::Value::EndOfMibView => {
                debug!("SNMP响应: EndOfMibView (MIB视图末尾)");
                Ok(format!(
                    "SNMP连接成功 ({}:{})，已到达MIB视图末尾",
                    params.ip, params.port
                ))
            }
            _ => Err(SnmpError::Message("响应为异常值".to_string())),
        }
    } else if let Some(s) = result.value.as_str() {
        Ok(s.to_string())
    } else {
        Err(SnmpError::Message(
            "响应格式不正确: 期望字符串类型".to_string(),
        ))
    }
}

/// 经 SNMP 获取的设备识别信息（sysDescr 推导 + sysName 主机名）。
#[derive(Debug, Clone)]
pub struct DeviceSnmpInfo {
    pub brand: String,
    pub model: String,
    /// 设备主机名（sysName），设备不支持该 OID 时为 None
    pub hostname: Option<String>,
}

pub async fn get_device_info_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<DeviceSnmpInfo, SnmpError> {
    let addr = format!("{}:{}", params.ip, params.port);
    let auth = build_auth(params).map_err(SnmpError::Message)?;
    let client = Client::builder(&addr, auth)
        .timeout(Duration::from_secs(5))
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let sys_descr_result = client
        .get(&oid!(1, 3, 6, 1, 2, 1, 1, 1, 0))
        .await
        .map_err(|e| SnmpError::Message(format!("SNMP请求失败: {}", format_snmp_error(e))))?;
    let sys_descr = sys_descr_result
        .value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| SnmpError::Message("响应格式不正确: 期望字符串类型".to_string()))?;

    // sysName（1.3.6.1.2.1.1.5.0）：部分设备不实现，异常响应时容忍为 None
    let hostname = match client.get(&oid!(1, 3, 6, 1, 2, 1, 1, 5, 0)).await {
        Ok(v) if !v.value.is_exception() => v
            .value
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => None,
    };

    Ok(DeviceSnmpInfo {
        brand: identify_vendor(&sys_descr),
        model: extract_model(&sys_descr),
        hostname,
    })
}

fn identify_vendor(sys_descr: &str) -> String {
    let lower = sys_descr.to_lowercase();

    if lower.contains("cisco") {
        return "Cisco".to_string();
    }
    if lower.contains("huawei") {
        return "Huawei".to_string();
    }
    if lower.contains("h3c") || lower.contains("3com") {
        return "H3C".to_string();
    }
    if lower.contains("juniper") {
        return "Juniper".to_string();
    }
    if lower.contains("dell") {
        return "Dell".to_string();
    }
    if lower.contains("hp") || lower.contains("hpe") || lower.contains("procurve") {
        return "HP/HPE".to_string();
    }
    if lower.contains("aruba") {
        return "Aruba".to_string();
    }
    if lower.contains("netgear") {
        return "Netgear".to_string();
    }
    if lower.contains("tp-link") || lower.contains("tplink") {
        return "TP-Link".to_string();
    }
    if lower.contains("linksys") {
        return "Linksys".to_string();
    }
    if lower.contains("ubiquiti") || lower.contains("ubnt") {
        return "Ubiquiti".to_string();
    }
    if lower.contains("mikrotik") {
        return "MikroTik".to_string();
    }
    if lower.contains("extreme") {
        return "Extreme Networks".to_string();
    }
    if lower.contains("alcatel") {
        return "Alcatel".to_string();
    }
    if lower.contains("zyxel") {
        return "ZyXEL".to_string();
    }
    if lower.contains("d-link") || lower.contains("dlink") {
        return "D-Link".to_string();
    }

    "Unknown".to_string()
}

fn extract_model(sys_descr: &str) -> String {
    let model: String = sys_descr
        .split_whitespace()
        .filter(|word| word.chars().any(|c| c.is_ascii_digit()))
        .take(2)
        .collect();

    if model.is_empty() {
        if sys_descr.len() > 50 {
            sys_descr.chars().take(50).collect()
        } else {
            sys_descr.to_string()
        }
    } else {
        model
    }
}

pub async fn get_device_ports_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<DevicePortCreate>, SnmpError> {
    let addr = format!("{}:{}", params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    let client = Client::builder(&addr, auth)
        .timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let mut ports = Vec::new();

    let if_descr_oid = oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2);

    let mut walk = client
        .walk(if_descr_oid)
        .map_err(|e| SnmpError::Message(format!("创建SNMP walk失败: {}", format_snmp_error(e))))?;

    while let Some(result) = walk.next().await {
        let vb = result
            .map_err(|e| SnmpError::Message(format!("SNMP walk失败: {}", format_snmp_error(e))))?;

        if vb.value.is_exception() {
            if matches!(vb.value, async_snmp::Value::EndOfMibView) {
                debug!("接口列表walk完成: 到达MIB视图末尾");
                break;
            }
            continue;
        }

        let oid_parts = vb.oid.arcs();
        let if_index = oid_parts.last().unwrap_or(&0).to_string();

        let port_number = vb
            .value
            .as_str()
            .map_or_else(|| if_index.clone(), std::string::ToString::to_string);

        ports.push(DevicePortCreate {
            port_number: port_number.clone(),
            port_name: None,
            port_type: None,
            vlan_id: None,
            status: None,
            speed: None,
            description: Some(port_number),
        });
    }

    debug!("获取接口列表完成: {} 条记录", ports.len());
    Ok(ports)
}

pub async fn test_snmp_connection_by_id(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
    AppJson(req): AppJson<SnmpTestRequest>,
) -> Result<Response, AppError> {
    let mut test_req = req;
    test_req.device_id = Some(device_id);
    test_snmp_connection(State(state), AppJson(test_req)).await
}

pub async fn test_snmp_connection(
    State(state): State<Arc<AppState>>,
    AppJson(req): AppJson<SnmpTestRequest>,
) -> Result<Response, AppError> {
    // 记录测试请求概要（Option 值先转为字符串以便日志参数化）
    let device_id_label = req.device_id.map(|d| d.to_string()).unwrap_or_default();
    let ip_label = req.ip_address.clone().unwrap_or_default();
    let version_label = req.snmp_version.clone().unwrap_or_default();
    log_info!(
        "log.device.snmp.test_request",
        device_id = device_id_label,
        ip = ip_label,
        version = version_label
    );

    let (ip, version, community, username, auth_proto, auth_pass, priv_proto, priv_pass, port) =
        if let Some(device_id) = req.device_id {
            let conn = state.pool()?.get_conn();
            let (switch, ip_address) = get_device_snmp_config(&conn, &device_id).await?;

            let creds = DecryptedSnmpCredentials::from_device_snmp_async(&switch).await?;

            let version = req.snmp_version.clone().unwrap_or(switch.snmp_version);
            let community = req.snmp_community.clone().or(creds.community);
            let username = req.snmp_username.clone().or(switch.snmp_username);
            let auth_proto = req.snmp_auth_protocol.clone().or(switch.snmp_auth_protocol);
            let auth_pass = req.snmp_auth_password.clone().or(creds.auth_password);
            let priv_proto = req.snmp_priv_protocol.clone().or(switch.snmp_priv_protocol);
            let priv_pass = req.snmp_priv_password.clone().or(creds.priv_password);
            let port = req.snmp_port.unwrap_or(switch.snmp_port);

            (
                req.ip_address.clone().or(ip_address),
                version,
                community,
                username,
                auth_proto,
                auth_pass,
                priv_proto,
                priv_pass,
                port,
            )
        } else {
            (
                req.ip_address.clone(),
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

    let ip = match ip {
        Some(ref s) if !s.is_empty() => s.clone(),
        _ => return Err(AppError::Validation(msg("server.device.snmp.ip_required"))),
    };

    // 验证目标 IP 不是私有/回环/链路本地/组播地址，防止 SSRF
    let parsed_ip: std::net::IpAddr = ip
        .parse()
        .map_err(|_| AppError::Validation(msg("server.device.snmp.ip_invalid").with("ip", &ip)))?;
    if parsed_ip.is_loopback() {
        return Err(AppError::Validation(msg(
            "server.device.snmp.loopback_forbidden",
        )));
    }
    if parsed_ip.is_multicast() {
        return Err(AppError::Validation(msg(
            "server.device.snmp.multicast_forbidden",
        )));
    }
    match parsed_ip {
        std::net::IpAddr::V4(v4) => {
            if v4.is_link_local() {
                return Err(AppError::Validation(msg(
                    "server.device.snmp.link_local_forbidden",
                )));
            }
            if v4.is_private() {
                log_warn!("log.device.snmp.private_ip_allowed", ip = ip);
            }
            let octets = v4.octets();
            if octets[0] == 169 && octets[1] == 254 && octets[2] == 169 && octets[3] == 254 {
                return Err(AppError::Validation(msg(
                    "server.device.snmp.metadata_endpoint_forbidden",
                )));
            }
        }
        std::net::IpAddr::V6(v6) => {
            // IPv6 没有与 IPv4 相同的私有/链路本地概念，但检查常用受限范围
            if v6.is_unique_local() {
                log_warn!("log.device.snmp.ula_ip_allowed", ip = ip);
            }
        }
    }

    let snmp_params = SnmpParamsLegacy {
        ip: ip.clone(),
        port,
        version: version.clone(),
        community: community.clone(),
        username: username.clone(),
        auth_proto: auth_proto.clone(),
        auth_pass: auth_pass.clone(),
        priv_proto: priv_proto.clone(),
        priv_pass: priv_pass.clone(),
        timeout_secs: 10,
    };

    // 记录测试参数（敏感凭据只记录是否设置，不记录明文）
    let community_label = community.as_ref().map(|_| "set").unwrap_or("none");
    let username_label = username.clone().unwrap_or_default();
    log_info!(
        "log.device.snmp.test_params",
        ip = ip,
        port = port,
        version = version,
        community = community_label,
        username = username_label
    );

    match test_snmp(&snmp_params, 5).await {
        Ok(sys_descr) => Ok(crate::error::ok_json(
            serde_json::json!({ "sysDescr": sys_descr }),
            "server.device.snmp.test_success",
        )),
        Err(e) => Err(AppError::Snmp(
            msg("server.device.snmp.test_failed").with("error", e),
        )),
    }
}

pub async fn get_device_info_snmp(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (switch, ip_address) =
        get_device_snmp_config(&state.pool()?.get_conn(), &device_id).await?;

    let ip_address =
        ip_address.ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;

    let snmp_params = switch.to_snmp_params_async(&ip_address).await?;

    match get_device_info_via_snmp(&snmp_params).await {
        Ok(info) => Ok(crate::error::ok_json(
            serde_json::json!({
                "brand": info.brand,
                "model": info.model,
                "hostname": info.hostname
            }),
            "server.device.snmp.info_retrieved",
        )),
        Err(e) => Err(AppError::Snmp(
            msg("server.device.snmp.info_fetch_failed").with("error", e),
        )),
    }
}

pub async fn get_device_ports_snmp(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let (switch, ip_address) =
        get_device_snmp_config(&state.pool()?.get_conn(), &device_id).await?;

    let ip_address =
        ip_address.ok_or_else(|| AppError::Validation(msg("server.device.no_ip_configured")))?;

    let snmp_params = switch.to_snmp_params_async(&ip_address).await?;

    match get_device_ports_via_snmp(&snmp_params).await {
        Ok(ports) => Ok(crate::error::ok_json(
            ports,
            "server.device.snmp.ports_retrieved",
        )),
        Err(e) => Err(AppError::Snmp(
            msg("server.device.snmp.switch_ports_fetch_failed").with("error", e),
        )),
    }
}
