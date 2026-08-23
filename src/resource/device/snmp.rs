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

#[cfg(test)]
mod tests {
    use super::*;
    use async_snmp::{ErrorStatus, WalkAbortReason};
    use std::net::SocketAddr;

    /// 构造 v2c 参数
    fn v2c_params(community: Option<&str>) -> SnmpParamsLegacy {
        SnmpParamsLegacy {
            ip: "192.0.2.1".to_string(),
            port: 161,
            version: "v2c".to_string(),
            community: community.map(str::to_string),
            username: None,
            auth_proto: None,
            auth_pass: None,
            priv_proto: None,
            priv_pass: None,
            timeout_secs: 1,
        }
    }

    /// 构造 v3 参数
    fn v3_params(
        username: Option<&str>,
        auth_proto: Option<&str>,
        priv_proto: Option<&str>,
    ) -> SnmpParamsLegacy {
        SnmpParamsLegacy {
            ip: "192.0.2.1".to_string(),
            port: 161,
            version: "v3".to_string(),
            community: None,
            username: username.map(str::to_string),
            auth_proto: auth_proto.map(str::to_string),
            auth_pass: Some("auth-pass".to_string()),
            priv_proto: priv_proto.map(str::to_string),
            priv_pass: Some("priv-pass".to_string()),
            timeout_secs: 1,
        }
    }

    // ==================== 厂商识别 ====================

    #[test]
    fn test_identify_vendor_known_brands() {
        let cases = [
            ("Cisco IOS Software", "Cisco"),
            ("Huawei Versatile Routing Platform", "Huawei"),
            ("H3C Comware Software", "H3C"),
            ("3Com SuperStack Switch", "H3C"),
            ("Juniper Networks Junos", "Juniper"),
            ("Dell EMC Networking", "Dell"),
            ("HP ProCurve Switch", "HP/HPE"),
            ("HPE OfficeConnect", "HP/HPE"),
            ("ProCurve J9021A", "HP/HPE"),
            ("ArubaOS-CX Switch", "Aruba"),
            ("NETGEAR ProSafe GS724T", "Netgear"),
            ("TP-Link JetStream T2600G", "TP-Link"),
            ("TPLink Web Smart Switch", "TP-Link"),
            ("Linksys LGS318", "Linksys"),
            ("Ubiquiti UniFi Switch", "Ubiquiti"),
            ("UBNT EdgeSwitch", "Ubiquiti"),
            ("MikroTik RouterOS", "MikroTik"),
            ("ExtremeXOS Switch", "Extreme Networks"),
            ("Alcatel-Lucent OS6850", "Alcatel"),
            ("ZyXEL GS1900", "ZyXEL"),
            ("D-Link DGS-1210", "D-Link"),
            ("Dlink Smart Switch", "D-Link"),
        ];
        for (sys_descr, expected) in cases {
            assert_eq!(
                identify_vendor(sys_descr),
                expected,
                "sysDescr={sys_descr} 应识别为 {expected}"
            );
        }
    }

    #[test]
    fn test_identify_vendor_case_insensitive_and_priority() {
        // 大小写不敏感
        assert_eq!(identify_vendor("CISCO IOS"), "Cisco");
        assert_eq!(identify_vendor("HUAWEI S5720"), "Huawei");
        // 优先级：cisco 先于 huawei 判定
        assert_eq!(identify_vendor("cisco device by huawei oem"), "Cisco");
        // 未知厂商
        assert_eq!(identify_vendor("Some Generic Device"), "Unknown");
        assert_eq!(identify_vendor(""), "Unknown");
    }

    // ==================== 型号提取 ====================

    #[test]
    fn test_extract_model_with_digits() {
        // 取前两个含数字的词（无分隔符拼接）
        assert_eq!(extract_model("Huawei S5720 5700EI"), "S57205700EI");
        // 仅一个含数字词
        assert_eq!(extract_model("Cisco IOS C2960 Software"), "C2960");
    }

    #[test]
    fn test_extract_model_without_digits() {
        // 无数字词时短串原样返回
        assert_eq!(extract_model("Generic Switch"), "Generic Switch");
        // 长串（超过 50 字符）截断为前 50 字符
        let long_descr = "X".repeat(80);
        let model = extract_model(&long_descr);
        assert_eq!(model.len(), 50, "超长描述应截断为 50 字符");
        assert_eq!(model, "X".repeat(50));
    }

    // ==================== 认证参数构造 ====================

    #[test]
    fn test_build_auth_v2c() {
        let auth =
            build_auth(&v2c_params(Some("public"))).unwrap_or_else(|e| panic!("v2c 构造失败: {e}"));
        assert_eq!(auth, Auth::v2c("public"));
    }

    #[test]
    fn test_build_auth_v1_uses_v2c_community_version() {
        // 记录现状：v1 分支复用 Auth::v2c 构造器，协议版本为 V2c（疑似生产缺陷）
        let mut params = v2c_params(Some("private"));
        params.version = "v1".to_string();
        let auth = build_auth(&params).unwrap_or_else(|e| panic!("v1 构造失败: {e}"));
        assert_eq!(auth, Auth::v2c("private"), "v1 分支当前退化为 v2c 构造");
    }

    #[test]
    fn test_build_auth_v2c_requires_community() {
        let err = build_auth(&v2c_params(None)).err().unwrap_or_default();
        assert!(
            err.contains("community"),
            "错误信息应说明缺少 community: {err}"
        );
    }

    #[test]
    fn test_build_auth_unknown_version_rejected() {
        let mut params = v2c_params(Some("public"));
        params.version = "v4".to_string();
        let err = build_auth(&params).err().unwrap_or_default();
        assert!(err.contains("版本"), "错误信息应说明版本不支持: {err}");
    }

    #[test]
    fn test_build_auth_v3_requires_username() {
        let err = build_auth(&v3_params(None, None, None))
            .err()
            .unwrap_or_default();
        assert!(err.contains("用户名"), "错误信息应说明缺少用户名: {err}");
    }

    #[test]
    fn test_build_auth_v3_no_auth_level() {
        // noAuthNoPriv：仅用户名
        let auth = build_auth(&v3_params(Some("readonly"), None, None))
            .unwrap_or_else(|e| panic!("v3 无认证构造失败: {e}"));
        let expected: Auth = Auth::usm("readonly").into();
        assert_eq!(auth, expected);
    }

    #[test]
    fn test_build_auth_v3_auth_only() {
        // authNoPriv：认证协议 + 密码，不配隐私协议
        let auth = build_auth(&v3_params(Some("admin"), Some("SHA-256"), None))
            .unwrap_or_else(|e| panic!("v3 authNoPriv 构造失败: {e}"));
        let expected: Auth = Auth::usm("admin")
            .auth(AuthProtocol::Sha256, "auth-pass")
            .into();
        assert_eq!(auth, expected);
    }

    #[test]
    fn test_build_auth_v3_auth_priv_all_protocols() {
        // authPriv：全部认证 × 隐私协议别名组合均应构造成功
        for auth_proto in [
            "MD5", "SHA", "SHA-1", "SHA1", "SHA-224", "SHA-256", "SHA-384", "SHA-512",
        ] {
            let params = v3_params(Some("admin"), Some(auth_proto), Some("DES"));
            assert!(
                build_auth(&params).is_ok(),
                "认证协议 {auth_proto} 应被支持"
            );
        }
        for priv_proto in [
            "DES", "3DES", "DES3", "AES", "AES-128", "AES128", "AES-192", "AES192", "AES-256",
            "AES256",
        ] {
            let params = v3_params(Some("admin"), Some("SHA"), Some(priv_proto));
            assert!(
                build_auth(&params).is_ok(),
                "隐私协议 {priv_proto} 应被支持"
            );
        }
        // 完整组合等值校验
        let auth = build_auth(&v3_params(Some("admin"), Some("MD5"), Some("AES-128")))
            .unwrap_or_else(|e| panic!("v3 authPriv 构造失败: {e}"));
        let expected: Auth = Auth::usm("admin")
            .auth_priv(
                AuthProtocol::Md5,
                "auth-pass",
                PrivProtocol::Aes128,
                "priv-pass",
            )
            .into();
        assert_eq!(auth, expected);
    }

    #[test]
    fn test_build_auth_v3_rejects_unsupported_protocols() {
        // 不支持的认证协议
        let err = build_auth(&v3_params(Some("admin"), Some("RC4"), None))
            .err()
            .unwrap_or_default();
        assert!(
            err.contains("认证协议"),
            "错误信息应指明认证协议不支持: {err}"
        );
        // 不支持的隐私协议（需同时给出认证协议与密码才进入隐私分支）
        let err = build_auth(&v3_params(Some("admin"), Some("SHA"), Some("RC4")))
            .err()
            .unwrap_or_default();
        assert!(
            err.contains("隐私协议"),
            "错误信息应指明隐私协议不支持: {err}"
        );
    }

    // ==================== 错误格式化 ====================

    fn test_target() -> SocketAddr {
        "192.0.2.10:161"
            .parse()
            .unwrap_or_else(|e| panic!("测试地址解析失败: {e}"))
    }

    #[test]
    fn test_format_snmp_error_variants() {
        // 超时
        let timeout = Error::Timeout {
            target: test_target(),
            elapsed: Duration::from_secs(5),
            retries: 3,
        };
        let text = format_snmp_error(Box::new(timeout));
        assert!(text.contains("连接超时"), "超时错误文案: {text}");
        assert!(text.contains("192.0.2.10:161"), "应包含目标地址: {text}");

        // 网络错误
        let network = Error::Network {
            target: test_target(),
            source: std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused"),
        };
        let text = format_snmp_error(Box::new(network));
        assert!(text.contains("网络错误"), "网络错误文案: {text}");

        // SNMP 协议错误
        let snmp_err = Error::Snmp {
            target: test_target(),
            status: ErrorStatus::NoSuchName,
            index: 2,
            oid: None,
        };
        let text = format_snmp_error(Box::new(snmp_err));
        assert!(text.contains("SNMP错误"), "协议错误文案: {text}");
        assert!(text.contains("NoSuchName"), "应包含错误状态: {text}");

        // 认证失败
        let auth_err = Error::Auth {
            target: test_target(),
        };
        assert!(format_snmp_error(Box::new(auth_err)).contains("认证失败"));

        // 响应格式错误
        let malformed = Error::MalformedResponse {
            target: test_target(),
        };
        assert!(format_snmp_error(Box::new(malformed)).contains("响应格式错误"));

        // Walk 中断
        let aborted = Error::WalkAborted {
            target: test_target(),
            reason: WalkAbortReason::Cycle,
        };
        let text = format_snmp_error(Box::new(aborted));
        assert!(text.contains("Walk中断"), "Walk 中断文案: {text}");

        // 配置错误
        let config = Error::Config("bad config".into());
        assert_eq!(format_snmp_error(Box::new(config)), "配置错误: bad config");

        // 无效 OID
        let invalid_oid = Error::InvalidOid("1.2.x".into());
        assert_eq!(format_snmp_error(Box::new(invalid_oid)), "无效OID: 1.2.x");

        // 兜底分支
        let closed = Error::Closed {
            target: test_target(),
        };
        assert!(format_snmp_error(Box::new(closed)).contains("未知错误"));
    }

    // ==================== SSRF 地址分类（不发起真实网络请求的拒绝分支） ====================

    /// 构造不依赖数据库的 AppState（连接池为 None，SNMP 测试走直接 IP 输入路径）
    fn make_state() -> Arc<AppState> {
        use crate::config::{
            Config, DatabaseConfig, InitConfig, JwtConfig, ListenConfig, RateLimitConfig,
            ServerConfig, SnmpConfig,
        };
        use ipma_scheduler::TaskRegistry;
        let config = Config {
            database: DatabaseConfig {
                host: "127.0.0.1".to_string(),
                port: 5432,
                database: "ipma_test".to_string(),
                username: "ipma".to_string(),
                password: String::new(),
                max_connections: 1,
                min_connections: 1,
                acquire_timeout_secs: 1,
                idle_timeout_secs: 1,
                max_lifetime_secs: 1,
                query_timeout_secs: 1,
                health_check_interval_secs: 1,
            },
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                host_ipv6: None,
                public_url: "http://127.0.0.1".to_string(),
                session_timeout: None,
                page_timeout: None,
                cors_allowed_origins: Vec::new(),
                allow_localhost_cors: false,
                listen: ListenConfig::default(),
            },
            jwt: JwtConfig {
                // 满足 32 字符强度要求即可，SNMP 测试不使用 JWT
                secret: "snmp-unit-test-secret-0123456789".to_string(),
                access_token_expiry: "15m".to_string(),
                refresh_token_expiry: "7d".to_string(),
            },
            init: InitConfig { enabled: false },
            i18n: None,
            rate_limit: RateLimitConfig::default(),
            snmp: SnmpConfig::default(),
        };
        let state = AppState::new(config, None, Arc::new(TaskRegistry::new()));
        let state = state.unwrap_or_else(|e| panic!("测试 AppState 构造失败: {e}"));
        Arc::new(state)
    }

    /// 发起 SNMP 连接测试并断言返回的错误消息 key
    async fn assert_snmp_ip_rejected(ip: Option<&str>, expected_key: &str) {
        let req = crate::models::SnmpTestRequest {
            device_id: None,
            ip_address: ip.map(str::to_string),
            snmp_version: Some("v2c".to_string()),
            snmp_community: Some("public".to_string()),
            snmp_username: None,
            snmp_auth_protocol: None,
            snmp_auth_password: None,
            snmp_priv_protocol: None,
            snmp_priv_password: None,
            snmp_port: Some(161),
        };
        let result = test_snmp_connection(State(make_state()), AppJson(req)).await;
        let err = result.err().unwrap_or_else(|| panic!("IP {ip:?} 应被拒绝"));
        assert!(
            matches!(err, AppError::Validation(_)),
            "应返回 Validation 错误，实际: {err}"
        );
        assert_eq!(
            err.message().key(),
            expected_key,
            "IP {ip:?} 的拒绝原因不符"
        );
    }

    #[tokio::test]
    async fn test_snmp_rejects_loopback_addresses() {
        // IPv4/IPv6 回环均禁止
        assert_snmp_ip_rejected(Some("127.0.0.1"), "server.device.snmp.loopback_forbidden").await;
        assert_snmp_ip_rejected(
            Some("127.255.255.254"),
            "server.device.snmp.loopback_forbidden",
        )
        .await;
        assert_snmp_ip_rejected(Some("::1"), "server.device.snmp.loopback_forbidden").await;
    }

    #[tokio::test]
    async fn test_snmp_rejects_multicast_addresses() {
        assert_snmp_ip_rejected(Some("224.0.0.1"), "server.device.snmp.multicast_forbidden").await;
        assert_snmp_ip_rejected(Some("239.1.1.1"), "server.device.snmp.multicast_forbidden").await;
        assert_snmp_ip_rejected(Some("ff02::1"), "server.device.snmp.multicast_forbidden").await;
    }

    #[tokio::test]
    async fn test_snmp_rejects_link_local_and_metadata_addresses() {
        // IPv4 链路本地（169.254.0.0/16）
        assert_snmp_ip_rejected(
            Some("169.254.1.1"),
            "server.device.snmp.link_local_forbidden",
        )
        .await;
        // 云元数据端点 169.254.169.254 亦属链路本地段：
        // 现状先命中 is_link_local 检查（元数据专用分支不可达，已记录为生产问题）
        assert_snmp_ip_rejected(
            Some("169.254.169.254"),
            "server.device.snmp.link_local_forbidden",
        )
        .await;
    }

    #[tokio::test]
    async fn test_snmp_rejects_invalid_or_missing_ip() {
        // 非法 IP 字符串
        assert_snmp_ip_rejected(Some("not-an-ip"), "server.device.snmp.ip_invalid").await;
        assert_snmp_ip_rejected(Some("999.1.1.1"), "server.device.snmp.ip_invalid").await;
        // 缺失或空 IP
        assert_snmp_ip_rejected(None, "server.device.snmp.ip_required").await;
        assert_snmp_ip_rejected(Some(""), "server.device.snmp.ip_required").await;
    }
}
