use std::time::Duration;

use actix_web::{HttpResponse, Result, web};
use async_snmp::{
    Auth, Client, Error, oid,
    v3::{AuthProtocol, PrivProtocol},
};
use thiserror::Error;
use tracing::debug;
use uuid::Uuid;

use crate::crypto::{decrypt_credential, decrypt_password};
use crate::db::DbPool;
use crate::models::{ApiResponse, SnmpTestRequest, SwitchPortCreate, SwitchWithParent};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SwitchForSnmp {
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

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SwitchForSnmpWithNetwork {
    pub id: Uuid,
    pub name: String,
    pub network_id: Option<Uuid>,
    pub snmp_version: String,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: i32,
}

impl SwitchForSnmp {
    #[must_use] 
    pub fn to_snmp_params(&self, ip_address: &str) -> SnmpParamsLegacy {
        let creds = DecryptedSnmpCredentials::from_switch_snmp(self);
        SnmpParamsLegacy {
            ip: ip_address.to_string(),
            port: self.snmp_port,
            version: self.snmp_version.clone(),
            community: creds.community,
            username: self.snmp_username.clone(),
            auth_proto: self.snmp_auth_protocol.clone(),
            auth_pass: creds.auth_password,
            priv_proto: self.snmp_priv_protocol.clone(),
            priv_pass: creds.priv_password,
        }
    }
}

impl SwitchForSnmpWithNetwork {
    #[must_use] 
    pub fn to_snmp_params(&self, ip_address: &str) -> SnmpParamsLegacy {
        let creds = DecryptedSnmpCredentials::from_switch_snmp_with_network(self);
        SnmpParamsLegacy {
            ip: ip_address.to_string(),
            port: self.snmp_port,
            version: self.snmp_version.clone(),
            community: creds.community,
            username: self.snmp_username.clone(),
            auth_proto: self.snmp_auth_protocol.clone(),
            auth_pass: creds.auth_password,
            priv_proto: self.snmp_priv_protocol.clone(),
            priv_pass: creds.priv_password,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecryptedSnmpCredentials {
    pub community: Option<String>,
    pub auth_password: Option<String>,
    pub priv_password: Option<String>,
}

impl DecryptedSnmpCredentials {
    #[must_use] 
    pub fn from_switch_snmp(switch: &SwitchForSnmp) -> Self {
        Self {
            community: decrypt_credential(switch.snmp_community.as_deref()),
            auth_password: decrypt_credential(switch.snmp_auth_password.as_deref()),
            priv_password: decrypt_credential(switch.snmp_priv_password.as_deref()),
        }
    }

    #[must_use] 
    pub fn from_switch_snmp_with_network(switch: &SwitchForSnmpWithNetwork) -> Self {
        Self {
            community: decrypt_credential(switch.snmp_community.as_deref()),
            auth_password: decrypt_credential(switch.snmp_auth_password.as_deref()),
            priv_password: decrypt_credential(switch.snmp_priv_password.as_deref()),
        }
    }
}

#[derive(Error, Debug)]
pub enum SnmpError {
    #[error("{0}")]
    Message(String),
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
}

pub fn decrypt_snmp_fields(data: &mut SwitchWithParent) {
    data.snmp_community = data.snmp_community.as_ref().map(|v| decrypt_password(v));
    data.snmp_auth_password = data
        .snmp_auth_password
        .as_ref()
        .map(|v| decrypt_password(v));
    data.snmp_priv_password = data
        .snmp_priv_password
        .as_ref()
        .map(|v| decrypt_password(v));
}

pub fn build_auth(params: &SnmpParamsLegacy) -> Result<Auth, String> {
    match params.version.as_str() {
        "v1" | "v2c" => {
            let community = params.community.as_deref().unwrap_or("public");
            Ok(Auth::v2c(community))
        }
        "v3" => {
            let username = params
                .username
                .as_deref()
                .ok_or_else(|| "SNMPv3需要用户名".to_string())?;
            let mut auth = Auth::usm(username);

            if let (Some(proto), Some(pass)) =
                (params.auth_proto.as_deref(), params.auth_pass.as_deref())
            {
                let auth_protocol = match proto {
                    "MD5" => AuthProtocol::Md5,
                    "SHA" | "SHA-1" | "SHA1" => AuthProtocol::Sha1,
                    "SHA-224" => AuthProtocol::Sha224,
                    "SHA-256" => AuthProtocol::Sha256,
                    "SHA-384" => AuthProtocol::Sha384,
                    "SHA-512" => AuthProtocol::Sha512,
                    _ => return Err(format!("不支持的认证协议: {proto}")),
                };
                auth = auth.auth(auth_protocol, pass);

                if let (Some(proto), Some(pass)) =
                    (params.priv_proto.as_deref(), params.priv_pass.as_deref())
                {
                    let priv_protocol = match proto {
                        "DES" => PrivProtocol::Des,
                        "3DES" | "DES3" => PrivProtocol::Des3,
                        "AES" | "AES-128" | "AES128" => PrivProtocol::Aes128,
                        "AES-192" | "AES192" => PrivProtocol::Aes192,
                        "AES-256" | "AES256" => PrivProtocol::Aes256,
                        _ => return Err(format!("不支持的隐私协议: {proto}")),
                    };
                    auth = auth.privacy(priv_protocol, pass);
                }
            }

            Ok(auth.into())
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
            format!(
                "SNMP错误 (目标: {target}, 状态: {status:?}, 索引: {index})"
            )
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

pub async fn test_snmp(params: &SnmpParamsLegacy) -> Result<String, SnmpError> {
    let addr = format!("{}:{}", params.ip, params.port);
    let timeout = Duration::from_secs(5);

    tracing::info!("[test_snmp] 开始连接: {}", addr);

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    tracing::info!("[test_snmp] 认证构建成功");

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

pub async fn get_switch_info_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<(String, String), SnmpError> {
    let sys_descr = test_snmp(params).await?;

    let vendor = identify_vendor(&sys_descr);
    let model = extract_model(&sys_descr);

    Ok((vendor, model))
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

pub async fn get_switch_ports_via_snmp(
    params: &SnmpParamsLegacy,
) -> Result<Vec<SwitchPortCreate>, SnmpError> {
    let addr = format!("{}:{}", params.ip, params.port);
    let timeout = Duration::from_secs(10);

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
            .as_str().map_or_else(|| if_index.clone(), std::string::ToString::to_string);

        ports.push(SwitchPortCreate {
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
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    req: web::Json<SnmpTestRequest>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();
    let mut test_req = req.into_inner();
    test_req.switch_id = Some(switch_id);
    test_snmp_connection(pool, web::Json(test_req)).await
}

pub async fn test_snmp_connection(
    pool: web::Data<DbPool>,
    req: web::Json<SnmpTestRequest>,
) -> Result<HttpResponse> {
    tracing::info!("[test_snmp] 收到的完整请求: {:?}", req);

    let (ip, version, community, username, auth_proto, auth_pass, priv_proto, priv_pass, port) =
        if let Some(switch_id) = req.switch_id {
            let switch = sqlx::query_as::<_, SwitchForSnmp>(
                r"SELECT 
                    id, name, snmp_version, snmp_community, 
                    snmp_username, snmp_auth_protocol, 
                    snmp_auth_password, snmp_priv_protocol, 
                    snmp_priv_password, snmp_port
                FROM switches WHERE id = $1",
            )
            .bind(switch_id)
            .fetch_optional(pool.get_conn())
            .await;

            match switch {
                Ok(Some(s)) => {
                    let ip_address: Option<String> = sqlx::query_scalar(
                        r"SELECT host(ip_address) FROM ips 
                           WHERE position_id = (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1)
                           ORDER BY created_at LIMIT 1",
                    )
                    .bind(switch_id)
                    .fetch_optional(pool.get_conn())
                    .await
                    .ok()
                    .flatten();

                    let creds = DecryptedSnmpCredentials::from_switch_snmp(&s);

                    let version = req.snmp_version.clone().unwrap_or(s.snmp_version);
                    let community = req.snmp_community.clone().or(creds.community);
                    let username = req.snmp_username.clone().or(s.snmp_username);
                    let auth_proto = req.snmp_auth_protocol.clone().or(s.snmp_auth_protocol);
                    let auth_pass = req.snmp_auth_password.clone().or(creds.auth_password);
                    let priv_proto = req.snmp_priv_protocol.clone().or(s.snmp_priv_protocol);
                    let priv_pass = req.snmp_priv_password.clone().or(creds.priv_password);
                    let port = req.snmp_port.unwrap_or(s.snmp_port);

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
                }
                Ok(None) => {
                    return Ok(
                        HttpResponse::NotFound().json(ApiResponse::<()>::error("交换机不存在"))
                    );
                }
                Err(e) => {
                    return Ok(HttpResponse::InternalServerError()
                        .json(ApiResponse::<()>::error(format!("查询交换机失败: {e}"))));
                }
            }
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
        _ => return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error("IP地址不能为空"))),
    };

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
    };

    tracing::info!(
        "[test_snmp] 接收到的参数: ip={}, port={}, version={}, community={:?}, username={:?}, auth_pass={:?}, priv_pass={:?}",
        ip,
        port,
        version,
        community,
        username,
        auth_pass,
        priv_pass
    );

    match test_snmp(&snmp_params).await {
        Ok(sys_descr) => Ok(HttpResponse::Ok().json(ApiResponse::success(
            serde_json::json!({ "sysDescr": sys_descr }),
            "SNMP连接测试成功",
        ))),
        Err(e) => Ok(HttpResponse::BadRequest()
            .json(ApiResponse::<()>::error(format!("SNMP连接测试失败: {e}")))),
    }
}

pub async fn get_switch_info_snmp(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let switch = sqlx::query_as::<_, SwitchForSnmp>(
        r"SELECT 
            id, name, snmp_version, snmp_community, 
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password, snmp_priv_protocol, 
            snmp_priv_password, snmp_port
        FROM switches WHERE id = $1",
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
                .json(ApiResponse::<()>::error(format!("查询交换机失败: {e}"))));
        }
    };

    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(ip_address) FROM ips 
           WHERE position_id = (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1)
           ORDER BY created_at LIMIT 1",
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

    let snmp_params = switch.to_snmp_params(ip_address);

    match get_switch_info_via_snmp(&snmp_params).await {
        Ok((vendor, model)) => Ok(HttpResponse::Ok().json(ApiResponse::success(
            serde_json::json!({ "vendor": vendor, "model": model }),
            "获取交换机信息成功",
        ))),
        Err(e) => Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "获取交换机信息失败: {e}"
            ))),
        ),
    }
}

pub async fn get_switch_ports_snmp(
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let switch_id = path.into_inner();

    let switch = sqlx::query_as::<_, SwitchForSnmp>(
        r"SELECT 
            id, name, snmp_version, snmp_community, 
            snmp_username, snmp_auth_protocol, 
            snmp_auth_password, snmp_priv_protocol, 
            snmp_priv_password, snmp_port
        FROM switches WHERE id = $1",
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
                .json(ApiResponse::<()>::error(format!("查询交换机失败: {e}"))));
        }
    };

    let ip_address: Option<String> = sqlx::query_scalar(
        r"SELECT host(ip_address) FROM ips 
           WHERE position_id = (SELECT id FROM positions WHERE device_type = 'switch' AND device_id = $1)
           ORDER BY created_at LIMIT 1",
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

    let snmp_params = switch.to_snmp_params(ip_address);

    match get_switch_ports_via_snmp(&snmp_params).await {
        Ok(ports) => {
            Ok(HttpResponse::Ok().json(ApiResponse::success(ports, "获取交换机端口信息成功")))
        }
        Err(e) => Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "获取交换机端口信息失败: {e}"
            ))),
        ),
    }
}
