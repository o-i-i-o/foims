//! SNMP 协议参数层：设备凭据解密、参数构造、鉴权组装、目标地址与错误文案。

pub mod config;

use std::time::Duration;

use async_snmp::{
    Auth, Client, Error, UsmConfig, oid,
    v3::{AuthProtocol, PrivProtocol},
};
use thiserror::Error;
use tracing::debug;
use uuid::Uuid;

use foims_common::AppError;
use foims_common::crypto::decrypt_credential_async;
use foims_common::log_info;

pub use config::{SwitchConfigError, get_device_ip_address, get_device_snmp_config};

/// SNMP 请求超时单一口径（秒）：设备参数构造（[`DeviceForSnmp::to_snmp_params_async`]）
/// 与轮询会话（agent-service 的 connect_snmp）共用，C9：消除 10s/5s 双源。
pub const SNMP_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeviceForSnmp {
    pub id: Uuid,
    pub name: String,
    // snmp_version/snmp_port 数据库列可空（仅带 DEFAULT），按可空声明（M-1）
    pub snmp_version: Option<String>,
    pub snmp_community: Option<String>,
    pub snmp_username: Option<String>,
    pub snmp_auth_protocol: Option<String>,
    pub snmp_auth_password: Option<String>,
    pub snmp_priv_protocol: Option<String>,
    pub snmp_priv_password: Option<String>,
    pub snmp_port: Option<i32>,
}

impl DeviceForSnmp {
    pub async fn to_snmp_params_async(
        &self,
        ip_address: &str,
    ) -> Result<SnmpParamsLegacy, AppError> {
        let creds = DecryptedSnmpCredentials::from_device_snmp_async(self).await?;
        Ok(SnmpParamsLegacy {
            ip: ip_address.to_string(),
            port: self.snmp_port.unwrap_or(161),
            version: self
                .snmp_version
                .clone()
                .unwrap_or_else(|| "v2c".to_string()),
            community: creds.community,
            username: self.snmp_username.clone(),
            auth_proto: self.snmp_auth_protocol.clone(),
            auth_pass: creds.auth_password,
            priv_proto: self.snmp_priv_protocol.clone(),
            priv_pass: creds.priv_password,
            // C9：超时取共享常量，与轮询路径同源
            timeout_secs: SNMP_TIMEOUT_SECS,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DecryptedSnmpCredentials {
    pub community: Option<String>,
    pub auth_password: Option<String>,
    pub priv_password: Option<String>,
}

/// 空白凭据归一：空串/全空白视为未配置（None）。
///
/// devices 表的密文列历史上可能被写入空串，解密后得到 "" 会被
/// v1/v2c 分支当作合法 community 直接发包（空团体名绕过鉴权语义），
/// 统一归一后由「未配置」显式报错，不静默降级。
fn normalize_blank(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

impl DecryptedSnmpCredentials {
    pub async fn from_device_snmp_async(switch: &DeviceForSnmp) -> Result<Self, AppError> {
        let community =
            normalize_blank(decrypt_credential_async(switch.snmp_community.as_deref()).await?);
        let auth_password =
            normalize_blank(decrypt_credential_async(switch.snmp_auth_password.as_deref()).await?);
        let priv_password =
            normalize_blank(decrypt_credential_async(switch.snmp_priv_password.as_deref()).await?);
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

/// 解析 SNMPv3 认证协议（复用库内 FromStr 别名表，错误文案保持中文）。
/// SNMP 域内唯一定义，agent-service 的 trap 接收器 USM 用户构建同样复用
pub fn parse_auth_protocol(proto: &str) -> Result<AuthProtocol, String> {
    proto
        .parse()
        .map_err(|_| format!("不支持的认证协议: {proto}"))
}

/// 解析 SNMPv3 隐私协议。
///
/// 0.18 起 AES-192/256 需显式选择密钥扩展（Blumenthal/Reeder）；
/// 旧别名 AES-192/AES192、AES-256/AES256 沿用 0.17 的 Blumenthal 扩展
/// 语义，保证存量配置行为不变；新别名（如 AES-256-CISCO）交给库解析。
/// SNMP 域内唯一定义，agent-service 的 trap 接收器 USM 用户构建同样复用
pub fn parse_priv_protocol(proto: &str) -> Result<PrivProtocol, String> {
    match proto.to_ascii_uppercase().as_str() {
        "AES-192" | "AES192" => Ok(PrivProtocol::Aes192Blumenthal),
        "AES-256" | "AES256" => Ok(PrivProtocol::Aes256Blumenthal),
        _ => proto
            .parse()
            .map_err(|_| format!("不支持的隐私协议: {proto}")),
    }
}

pub fn build_auth(params: &SnmpParamsLegacy) -> Result<Auth, String> {
    match params.version.as_str() {
        "v1" => {
            let community = params
                .community
                .as_deref()
                .ok_or_else(|| "SNMP v1/v2c 需要配置 community 字符串".to_string())?;
            // v1 真实使用 v1 协议版本构造（此前误复用 v2c，v1 选项形同虚设，S-4）
            Ok(Auth::v1(community))
        }
        "v2c" => {
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
            // 0.18 起 USM 构建器为 UsmConfig，auth/auth_priv 会立即校验
            // 协议支持性与密码长度，失败需转换为既有 String 错误
            let usm = UsmConfig::new(username.to_string());

            // 半配置凭据一律显式报错，禁止静默降级安全级别
            //（noAuthNoPriv 仅限四项全缺省；协议在而密码缺失多为解密
            // 失败/存量脏数据，降级发包会以明文绕过设备侧 USM）
            match (params.auth_proto.as_deref(), params.auth_pass.as_deref()) {
                (Some(_), None) => return Err("SNMPv3认证协议已配置但认证密码缺失".to_string()),
                (None, Some(_)) => return Err("SNMPv3认证密码已配置但认证协议缺失".to_string()),
                _ => {}
            }
            match (params.priv_proto.as_deref(), params.priv_pass.as_deref()) {
                (Some(_), None) => return Err("SNMPv3隐私协议已配置但隐私密码缺失".to_string()),
                (None, Some(_)) => return Err("SNMPv3隐私密码已配置但隐私协议缺失".to_string()),
                _ => {}
            }

            let usm = match (params.auth_proto.as_deref(), params.auth_pass.as_deref()) {
                (Some(proto), Some(auth_pass)) => {
                    let auth_protocol = parse_auth_protocol(proto)?;

                    if let (Some(priv_proto), Some(priv_pass)) =
                        (params.priv_proto.as_deref(), params.priv_pass.as_deref())
                    {
                        usm.auth_priv(
                            auth_protocol,
                            auth_pass,
                            parse_priv_protocol(priv_proto)?,
                            priv_pass,
                        )
                        .map_err(|e| format!("SNMPv3隐私配置无效: {e}"))?
                    } else {
                        usm.auth(auth_protocol, auth_pass)
                            .map_err(|e| format!("SNMPv3认证配置无效: {e}"))?
                    }
                }
                // 隐私已配置而认证整体缺省：authPriv 必须建立在认证
                // 之上，禁止静默丢弃隐私配置降级 noAuthNoPriv（B1）
                (None, None) if params.priv_proto.is_some() => {
                    return Err("SNMPv3隐私已配置但认证协议缺失".to_string());
                }
                // 四项全缺省：noAuthNoPriv（显式配置的空密文已被
                // normalize_blank 归一为缺失，进入本分支即真实未配置）
                _ => usm,
            };

            Ok(Auth::from(usm))
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

/// 构造 SNMP 目标地址串：IPv6 必须包裹方括号（如 `[2001:db8::1]:161`），
/// 直接 `ip:port` 拼接对 IPv6 无效（security-review S-2）
#[must_use]
pub fn snmp_target(ip: &str, port: i32) -> String {
    if ip.contains(':') && !ip.starts_with('[') {
        format!("[{ip}]:{port}")
    } else {
        format!("{ip}:{port}")
    }
}

/// 按字符数把文本截断到列宽以内，超宽时输出 warn 日志。
///
/// SNMP 采集的 ifName/local_port 可能超过库列宽（interface/local_port
/// 均为 VARCHAR(50)）：截断入库比整条拒绝更贴合采集场景
///（整条拒绝会让该行计入失败并丢失其余有效字段）。
#[must_use]
pub fn truncate_to_column_width(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    tracing::warn!(
        "SNMP 采集文本超列宽已截断: 原长度 {} 字符，保留前 {} 字符",
        char_count,
        max_chars
    );
    value.chars().take(max_chars).collect()
}

/// 可选文本按列宽截断（入库前统一口径，None 透传）。
#[must_use]
pub fn truncate_opt_to_column_width(value: Option<&str>, max_chars: usize) -> Option<String> {
    value.map(|v| truncate_to_column_width(v, max_chars))
}

/// 异常语义字面量（B3）：部分 v1/劣质固件会把异常语义写成
/// DisplayString 文本而非 BER exception 标签，逃过 is_exception
/// 类型判断后会以真实名称入库，字符串采集入口统一按此丢弃。
#[must_use]
pub fn is_exception_literal(text: &str) -> bool {
    matches!(
        text.trim()
            .to_ascii_lowercase()
            .replace(['_', '-'], "")
            .as_str(),
        "nosuchobject" | "nosuchinstance" | "nosuchname" | "endofmibview"
    )
}

/// SNMP 字符串字段归一（B3）：trim + 空串/异常字面量视为缺失。
#[must_use]
pub fn clean_snmp_text(value: Option<&str>) -> Option<String> {
    let text = value.map(str::trim).filter(|s| !is_exception_literal(s))?;
    (!text.is_empty()).then(|| text.to_string())
}

/// 归一 MAC：剔除 `:`/`-`/`.`/空格并转大写，返回十六进制串（比较用，分隔符无关）。
///
/// MAC 列在不同写入路径（SNMP 采集大写冒号 / 人工录入小写无分隔）下
/// 形态不一，所有跨表 MAC 比对（定位、已知集合判定）先经本函数归一。
#[must_use]
pub fn normalize_mac(mac: &str) -> String {
    mac.chars()
        .filter(|c| !matches!(c, ':' | '-' | ' ' | '.'))
        .flat_map(|c| c.to_uppercase())
        .collect()
}

/// 连通性探测：GET sysDescr，超时取 `params.timeout_secs`（单一超时口径，
/// 禁止调用侧再传第二超时参数造成双源）
pub async fn test_snmp(params: &SnmpParamsLegacy) -> Result<String, SnmpError> {
    let addr = snmp_target(&params.ip, params.port);
    let timeout = Duration::from_secs(params.timeout_secs);

    log_info!(
        "log.device.snmp.test_connecting",
        addr = addr,
        timeout = params.timeout_secs
    );

    let auth = build_auth(params).map_err(SnmpError::Message)?;

    log_info!("log.device.snmp.auth_built");

    debug!("连接SNMP设备: {} (版本: {})", addr, params.version);

    let client = Client::builder(&addr, auth)
        .construction_timeout(timeout)
        .request_timeout(timeout)
        .connect()
        .await
        .map_err(|e| SnmpError::Message(format!("创建SNMP会话失败: {}", format_snmp_error(e))))?;

    let result = client
        .get(&oid!(1, 3, 6, 1, 2, 1, 1, 1, 0))
        .await
        .map_err(|e| SnmpError::Message(format!("SNMP请求失败: {}", format_snmp_error(e))))?;

    // 0.18 起单 OID GET 返回变量绑定列表，取首个绑定的值
    let value = result
        .varbinds
        .first()
        .map(|vb| &vb.value)
        .ok_or_else(|| SnmpError::Message("响应不包含任何变量绑定".to_string()))?;

    if value.is_exception() {
        match value {
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
    } else if let Some(s) = value.as_str() {
        Ok(s.to_string())
    } else {
        Err(SnmpError::Message(
            "响应格式不正确: 期望字符串类型".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_snmp::ResponseMetadata;
    use async_snmp::message::SecurityLevel;
    use async_snmp::{CommunityVersion, ErrorStatus, WalkAbortReason};
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
            // 密码跟随协议联动：协议缺省则密码也缺省（否则与 B1
            // 半配置凭据校验矛盾，无法构造 authOnly/noAuth 等组合）
            auth_pass: auth_proto.map(|_| "auth-pass".to_string()),
            priv_proto: priv_proto.map(str::to_string),
            priv_pass: priv_proto.map(|_| "priv-pass".to_string()),
            timeout_secs: 1,
        }
    }

    // ==================== 认证参数构造 ====================

    #[test]
    fn test_build_auth_v2c() {
        let auth =
            build_auth(&v2c_params(Some("public"))).unwrap_or_else(|e| panic!("v2c 构造失败: {e}"));
        let Auth::Community { version, community } = auth else {
            panic!("v2c 应构造 Community 配置");
        };
        assert_eq!(version, CommunityVersion::V2c);
        assert!(community.matches(b"public"));
    }

    #[test]
    fn test_build_auth_v1_uses_v1_protocol() {
        // S-4 修复：v1 分支真实使用 Auth::v1（此前误复用 v2c 构造）
        let mut params = v2c_params(Some("private"));
        params.version = "v1".to_string();
        let auth = build_auth(&params).unwrap_or_else(|e| panic!("v1 构造失败: {e}"));
        let Auth::Community { version, community } = auth else {
            panic!("v1 应构造 Community 配置");
        };
        assert_eq!(version, CommunityVersion::V1, "v1 应使用 v1 协议构造");
        assert!(community.matches(b"private"));
    }

    #[test]
    fn test_snmp_target_wraps_ipv6_in_brackets() {
        // IPv6 目标必须包裹方括号，IPv4 原样拼接（S-2）
        assert_eq!(snmp_target("192.0.2.1", 161), "192.0.2.1:161");
        assert_eq!(snmp_target("2001:db8::1", 161), "[2001:db8::1]:161");
        assert_eq!(snmp_target("fe80::1%25eth0", 161), "[fe80::1%25eth0]:161");
        // 已带括号的输入不重复包裹
        assert_eq!(snmp_target("[2001:db8::1]", 161), "[2001:db8::1]:161");
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
        let Auth::Usm(usm) = auth else {
            panic!("v3 无认证应构造 USM 配置");
        };
        assert_eq!(usm.username().as_ref(), "readonly".as_bytes());
        assert_eq!(usm.security_level(), SecurityLevel::NoAuthNoPriv);
    }

    #[test]
    fn test_build_auth_v3_auth_only() {
        // authNoPriv：认证协议 + 密码，不配隐私协议
        let auth = build_auth(&v3_params(Some("admin"), Some("SHA-256"), None))
            .unwrap_or_else(|e| panic!("v3 authNoPriv 构造失败: {e}"));
        let Auth::Usm(usm) = auth else {
            panic!("v3 authNoPriv 应构造 USM 配置");
        };
        assert_eq!(usm.auth_protocol(), Some(AuthProtocol::Sha256));
        assert_eq!(usm.security_level(), SecurityLevel::AuthNoPriv);
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
        // 完整组合字段校验（0.18 起 Auth 不再实现 PartialEq，逐字段断言）
        let auth = build_auth(&v3_params(Some("admin"), Some("MD5"), Some("AES-128")))
            .unwrap_or_else(|e| panic!("v3 authPriv 构造失败: {e}"));
        let Auth::Usm(usm) = auth else {
            panic!("v3 authPriv 应构造 USM 配置");
        };
        assert_eq!(usm.auth_protocol(), Some(AuthProtocol::Md5));
        assert_eq!(usm.priv_protocol(), Some(PrivProtocol::Aes128));
        assert_eq!(usm.security_level(), SecurityLevel::AuthPriv);

        // 旧别名 AES-192/AES-256 保持 0.17 语义，映射到 Blumenthal 扩展
        for (alias, expected) in [
            ("AES-192", PrivProtocol::Aes192Blumenthal),
            ("AES256", PrivProtocol::Aes256Blumenthal),
            // 新显式别名应能选中 Reeder 扩展
            ("AES-256-CISCO", PrivProtocol::Aes256Reeder),
        ] {
            let auth = build_auth(&v3_params(Some("admin"), Some("SHA"), Some(alias)))
                .unwrap_or_else(|e| panic!("别名 {alias} 构造失败: {e}"));
            let Auth::Usm(usm) = auth else {
                panic!("别名 {alias} 应构造 USM 配置");
            };
            assert_eq!(usm.priv_protocol(), Some(expected), "别名 {alias}");
        }
    }

    #[test]
    fn test_build_auth_v3_rejects_unsupported_protocols() {
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

    #[test]
    fn test_build_auth_v3_rejects_partial_credentials() {
        // B1：半配置凭据显式报错，禁止静默降级 noAuthNoPriv/authNoPriv
        // 认证协议在而密码缺失
        let mut params = v3_params(Some("admin"), Some("SHA-256"), None);
        params.auth_pass = None;
        let err = build_auth(&params).err().unwrap_or_default();
        assert!(err.contains("认证密码缺失"), "应报认证密码缺失: {err}");
        // 隐私协议在而隐私密码缺失
        let mut params = v3_params(Some("admin"), Some("SHA-256"), Some("AES-128"));
        params.priv_pass = None;
        let err = build_auth(&params).err().unwrap_or_default();
        assert!(err.contains("隐私密码缺失"), "应报隐私密码缺失: {err}");
        // 隐私两项齐但认证两项齐缺：不支持的降级组合报错
        let mut params = v3_params(Some("admin"), None, Some("AES-128"));
        params.auth_pass = None;
        let err = build_auth(&params).err().unwrap_or_default();
        assert!(err.contains("认证协议缺失"), "应报认证协议缺失: {err}");
    }

    #[test]
    fn test_normalize_blank_treats_whitespace_as_unset() {
        // 空白密文解密产物归一为 None（A3 根治：避免空串绕过鉴权语义）
        assert_eq!(normalize_blank(Some("".to_string())), None);
        assert_eq!(normalize_blank(Some("   ".to_string())), None);
        assert_eq!(normalize_blank(None), None);
        assert_eq!(
            normalize_blank(Some("public".to_string())).as_deref(),
            Some("public")
        );
    }

    #[test]
    fn test_normalize_mac_strips_separators_and_uppercases() {
        // 跨表 MAC 比对统一口径：分隔符无关、大小写无关
        assert_eq!(normalize_mac("aa:bb:cc:dd:ee:ff"), "AABBCCDDEEFF");
        assert_eq!(normalize_mac("AA-BB-CC-DD-EE-FF"), "AABBCCDDEEFF");
        assert_eq!(normalize_mac("aa.bb.cc.dd.ee.ff"), "AABBCCDDEEFF");
        assert_eq!(normalize_mac("  aabbccddeeFF "), "AABBCCDDEEFF");
    }

    #[test]
    fn test_clean_snmp_text_rejects_exception_literals() {
        // B3：异常字面量（含下划线/连字符/大小写变体）在字符串入口丢弃
        assert!(is_exception_literal("noSuchObject"));
        assert!(is_exception_literal(" NoSuch_Instance "));
        assert!(is_exception_literal("no-such-name"));
        assert!(is_exception_literal("endOfMibView"));
        assert!(!is_exception_literal("GigabitEthernet0/1"));
        // 归一：trim + 空串/异常字面量 → None
        assert_eq!(
            clean_snmp_text(Some("  ge0/0/1 ")).as_deref(),
            Some("ge0/0/1")
        );
        assert_eq!(clean_snmp_text(Some("   ")), None);
        assert_eq!(clean_snmp_text(Some("noSuchObject")), None);
        assert_eq!(clean_snmp_text(None), None);
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
            metadata: Box::new(ResponseMetadata::default()),
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
}
