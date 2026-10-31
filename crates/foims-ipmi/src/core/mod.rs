//! IPMI 域核心：设备凭据行模型、RMCP+ 会话参数构造、连通性探测与错误文案。
//!
//! 依赖方向：仅使用 ruipmi 协议栈与 foims-common 加解密，不感知资源/服务域。

pub mod config;

use ruipmi::{AuthAlg, CipherSuite, CryptAlg, IntegrityAlg, IpmiClient, IpmiError};
use thiserror::Error;
use uuid::Uuid;

use foims_common::AppError;
use foims_common::crypto::decrypt_credential_async;
use foims_common::log_info;

pub use config::{IpmiConfigError, get_device_ip_address, get_device_ipmi_config};

/// IPMI LANplus 会话端口：ruipmi `IPMI_LANPLUS_PORT` 硬编码 623，不可配置。
/// `devices.ipmi_port` 列仅承载展示语义，采集/探测侧一律用本常量。
pub const IPMI_LAN_PORT: i32 = 623;

/// IPMI 认证算法（对应 RAKP 完整性/加密套件）：sha256=套件 17（默认），
/// sha1=套件 3（老固件兼容回落）。白名单与 DB CHECK 同口径。
pub const IPMI_AUTH_SHA256: &str = "sha256";
pub const IPMI_AUTH_SHA1: &str = "sha1";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeviceForIpmi {
    pub id: Uuid,
    pub name: String,
    // ipmi_port/ipmi_auth_algo 数据库列可空（仅带 DEFAULT），按可空声明（M-1）
    pub ipmi_username: Option<String>,
    pub ipmi_password: Option<String>,
    pub ipmi_port: Option<i32>,
    pub ipmi_auth_algo: Option<String>,
}

impl DeviceForIpmi {
    /// 设备行转 IPMI 会话参数：口令解密后注入；username 与解密后 password
    /// 均非空才视为「已配置」（返回 Some），任一缺失/空白返回 None（未配置）。
    pub async fn to_ipmi_params_async(
        &self,
        ip_address: &str,
    ) -> Result<Option<IpmiParams>, AppError> {
        let creds = DecryptedIpmiCredentials::from_device_ipmi_async(self).await?;
        let username = creds.username.filter(|v| !v.trim().is_empty());
        let password = creds.password.filter(|v| !v.trim().is_empty());
        match (username, password) {
            (Some(username), Some(password)) => Ok(Some(IpmiParams {
                ip: ip_address.to_string(),
                username,
                password,
                auth_algo: self
                    .ipmi_auth_algo
                    .clone()
                    .unwrap_or_else(|| IPMI_AUTH_SHA256.to_string()),
            })),
            _ => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecryptedIpmiCredentials {
    pub username: Option<String>,
    pub password: Option<String>,
}

impl DecryptedIpmiCredentials {
    pub async fn from_device_ipmi_async(device: &DeviceForIpmi) -> Result<Self, AppError> {
        // username 明文存储，仅做空白归一；password 为 AES-GCM 密文，解密后归一
        let username = device
            .ipmi_username
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string);
        let password =
            normalize_blank(decrypt_credential_async(device.ipmi_password.as_deref()).await?);
        Ok(Self { username, password })
    }
}

/// 空白凭据归一：空串/全空白视为未配置（None）。与 SNMP 域同口径——密文列
/// 历史可能被写入空串，解密得 "" 会绕过鉴权语义，统一归一后显式判未配置。
fn normalize_blank(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

#[derive(Error, Debug)]
pub enum IpmiErrorExt {
    #[error("{0}")]
    Message(String),
}

impl From<AppError> for IpmiErrorExt {
    fn from(e: AppError) -> Self {
        IpmiErrorExt::Message(e.to_string())
    }
}

impl From<IpmiError> for IpmiErrorExt {
    fn from(e: IpmiError) -> Self {
        IpmiErrorExt::Message(format_ipmi_error(&e))
    }
}

/// IPMI 会话连接参数（凭据已解密为明文，仅在本 crate 内存中流转，不外发）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpmiParams {
    pub ip: String,
    pub username: String,
    pub password: String,
    /// sha1 | sha256
    pub auth_algo: String,
}

impl IpmiParams {
    /// 按 `auth_algo` 构造 RMCP+ 密码套件：sha256 → 套件 17（HMAC-SHA256 +
    /// HMAC-SHA256-128 + AES-CBC-128），其余（含 sha1）→ 套件 3 风格
    ///（HMAC-SHA1 + HMAC-SHA1-96 + AES-CBC-128）。
    #[must_use]
    pub fn cipher_suite(&self) -> CipherSuite {
        if self.auth_algo == IPMI_AUTH_SHA1 {
            CipherSuite {
                authentication: AuthAlg::HmacSha1,
                integrity: IntegrityAlg::HmacSha1_96,
                confidentiality: CryptAlg::AesCbc128,
            }
        } else {
            CipherSuite {
                authentication: AuthAlg::HmacSha256,
                integrity: IntegrityAlg::HmacSha256_128,
                confidentiality: CryptAlg::AesCbc128,
            }
        }
    }
}

/// `ruipmi` 错误 → 面向用户的中文文案（未列出的变体归「未知错误」兜底）。
#[must_use]
pub fn format_ipmi_error(e: &IpmiError) -> String {
    match e {
        IpmiError::Socket(msg) => format!("网络/套接字错误: {msg}"),
        IpmiError::Timeout => "连接超时（BMC 无响应或会话端口不可达）".to_string(),
        IpmiError::BadResponse => "响应格式错误".to_string(),
        IpmiError::CipherMismatch => "密码套件不匹配（尝试切换认证算法 sha1/sha256）".to_string(),
        IpmiError::AuthFailed => "认证失败（用户名或口令错误，特权级别不足）".to_string(),
        IpmiError::DecryptFailed => "解密失败（会话加密不一致）".to_string(),
        IpmiError::InvalidState(state) => format!("会话状态无效: {state}"),
    }
}

/// 建立 RMCP+ 会话（Open Session + RAKP1-4 握手）。调用方负责结束后 `close`。
pub async fn open_session(params: &IpmiParams) -> Result<IpmiClient, IpmiError> {
    let mut client = IpmiClient::new(
        &params.ip,
        &params.username,
        &params.password,
        Some(params.cipher_suite()),
        None,
        None,
    )
    .await?;
    client.connect().await?;
    Ok(client)
}

/// 按字符数把文本截断到列宽以内，超宽时输出 warn 日志（与 SNMP 域同口径）。
#[must_use]
pub fn truncate_ipmi_text(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars {
        return value.to_string();
    }
    tracing::warn!(
        "IPMI 采集文本超列宽已截断: 原长度 {} 字符，保留前 {} 字符",
        char_count,
        max_chars
    );
    value.chars().take(max_chars).collect()
}

/// 剥离 IPMI 响应帧的完成码：`request()` 返回 `[cc, data...]`，
/// 完成码非 0 视为设备侧错误，返回可阅读文案；成功返回 data 切片。
pub fn take_response(resp: &[u8]) -> Result<&[u8], IpmiErrorExt> {
    let cc = resp
        .first()
        .ok_or_else(|| IpmiErrorExt::Message("空响应".to_string()))?;
    if *cc != 0 {
        return Err(IpmiErrorExt::Message(format!("设备返回完成码 0x{cc:02X}")));
    }
    Ok(&resp[1..])
}

/// Get Device ID 解析结果（用于连通性回显与轮询厂商串）。
#[derive(Debug, Clone)]
pub struct DeviceIdInfo {
    pub device_id: u8,
    pub manuf_id: u32,
    pub product_id: u16,
    pub fw_revision: String,
    pub ipmi_version: u8,
}

/// 解析 Get Device ID 响应 data 段（IPMI 规范 §20_1）：
/// `[device_id, dev_rev, fw_major(BCD), fw_minor(低6位)+aux, ipmi_ver, support,
/// manuf_id(3,LE), product_id(2,LE), aux(4)]`。字段不足时按已读到的部分返回。
fn parse_device_id(data: &[u8]) -> DeviceIdInfo {
    let device_id = data.first().copied().unwrap_or(0);
    let ipmi_version = data.get(4).copied().unwrap_or(0);
    let fw_major = data.get(2).copied().unwrap_or(0);
    let fw_minor = data.get(3).map(|b| b & 0x3F).unwrap_or(0);
    let manuf_id = data
        .get(6..9)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], 0]))
        .unwrap_or(0);
    let product_id = data
        .get(9..11)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .unwrap_or(0);
    DeviceIdInfo {
        device_id,
        manuf_id,
        product_id,
        fw_revision: format!("{fw_major:#04X}.{fw_minor:02}"),
        ipmi_version,
    }
}

/// 厂商可读串：写入 monitored_hosts.os（IPMI 无 OS 语义，以 BMC 厂商标识占位）。
impl DeviceIdInfo {
    #[must_use]
    pub fn vendor_label(&self) -> String {
        format!(
            "IPMI BMC 厂商 0x{:06X} 产品 0x{:04X}",
            self.manuf_id, self.product_id
        )
    }
}

/// 连通性探测：建会话 → Get Device ID → 关闭。成功返回设备 ID 摘要文本。
pub async fn test_ipmi(params: &IpmiParams) -> Result<String, IpmiErrorExt> {
    log_info!(
        "log.device.ipmi.test_connecting",
        ip = params.ip.clone(),
        auth_algo = params.auth_algo.clone()
    );
    let mut client = open_session(params).await?;
    let result = async {
        let resp = client.get_device_id().await?;
        let data = take_response(&resp)?.to_vec();
        let info = parse_device_id(&data);
        Ok::<String, IpmiErrorExt>(format!(
            "IPMI 连接成功 ({}), Device ID 0x{:02X}, 厂商 0x{:06X}, 产品 0x{:04X}, 固件 {}, IPMI 版本 0x{:02X}",
            params.ip, info.device_id, info.manuf_id, info.product_id, info.fw_revision, info.ipmi_version
        ))
    }
    .await;
    let _ = client.close().await;
    result
}

/// 读取设备 Get Device ID 信息（供轮询写入厂商串与产品标识）。
pub async fn read_device_id(client: &mut IpmiClient) -> Result<DeviceIdInfo, IpmiErrorExt> {
    let resp = client.get_device_id().await?;
    let data = take_response(&resp)?;
    Ok(parse_device_id(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(algo: &str) -> IpmiParams {
        IpmiParams {
            ip: "192.0.2.1".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
            auth_algo: algo.to_string(),
        }
    }

    #[test]
    fn cipher_suite_sha256_maps_to_suite_17() {
        let suite = params("sha256").cipher_suite();
        assert!(matches!(suite.authentication, AuthAlg::HmacSha256));
        assert!(matches!(suite.integrity, IntegrityAlg::HmacSha256_128));
        assert!(matches!(suite.confidentiality, CryptAlg::AesCbc128));
    }

    #[test]
    fn cipher_suite_sha1_maps_to_suite_3_style() {
        let suite = params("sha1").cipher_suite();
        assert!(matches!(suite.authentication, AuthAlg::HmacSha1));
        assert!(matches!(suite.integrity, IntegrityAlg::HmacSha1_96));
    }

    #[test]
    fn parse_device_id_reads_manuf_and_product() {
        // 完成码已剥离的 data 段：device_id=0x20, fw_major=0x02, fw_minor=0x05,
        // ipmi_ver=0x02, manuf_id=0x000157(LE), product_id=0x0001
        let data = [
            0x20, 0x00, 0x02, 0x05, 0x02, 0xC0, 0x57, 0x01, 0x00, 0x01, 0x00, 0, 0, 0, 0,
        ];
        let info = parse_device_id(&data);
        assert_eq!(info.device_id, 0x20);
        assert_eq!(info.manuf_id, 0x000157);
        assert_eq!(info.product_id, 0x0001);
        assert_eq!(info.ipmi_version, 0x02);
    }

    #[test]
    fn take_response_rejects_nonzero_completion_code() {
        let resp = [0xC1, 0x00];
        let err = take_response(&resp)
            .err()
            .unwrap_or_else(|| panic!("非零完成码应报错"));
        assert!(
            matches!(err, IpmiErrorExt::Message(_)),
            "应返回 Message 错误，实际: {err:?}"
        );
    }

    #[test]
    fn take_response_returns_data_on_success() {
        let resp = [0x00, 0xAA, 0xBB];
        let data = take_response(&resp).unwrap_or_else(|e| panic!("成功码应通过: {e:?}"));
        assert_eq!(data, [0xAA, 0xBB]);
    }
}
