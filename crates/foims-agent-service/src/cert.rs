//! Agent 服务端证书物料（设计 docs/agent-design.md §3.1）。
//!
//! 站点 CA（/etc/ssl/foims-ca/ca.pem + ca.key）存在时，为 QUIC 监听端签发
//! server 证书（serverAuth EKU + 本机全部网卡 IP 的 SAN），为 agent 签发共享
//! client 证书（clientAuth EKU，随下载包分发作为 mTLS 上报身份）。
//! 四个物料文件齐备且 `agent_ca_fingerprint` 指纹文件与当前站点 CA 一致时
//! 直接复用，不重复签发；站点 CA 轮换（重新 init）后指纹不匹配，自动重签
//! 全部物料并更新指纹文件，避免旧 CA 签发的证书导致 agent mTLS 握手失败。
//!
//! x509-management 的 `generate_certificate` 将 serverAuth EKU 硬编码
//! （面向浏览器/服务端证书场景），无法签发 clientAuth 证书，故此处直接
//! 用 rcgen 手写签发，不改共享 crate。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};

use rcgen::{
    CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, SanType,
};
use sha2::{Digest, Sha256};

/// Agent 服务端证书物料目录
pub const AGENT_CERT_DIR: &str = "/etc/ssl/foims-certs";
/// QUIC 监听端证书文件名
pub const SERVER_CERT: &str = "agent_cert.pem";
/// QUIC 监听端私钥文件名
pub const SERVER_KEY: &str = "agent_cert.key";
/// Agent 客户端证书文件名（随下载包分发）
pub const CLIENT_CERT: &str = "agent_client_cert.pem";
/// Agent 客户端私钥文件名（随下载包分发）
pub const CLIENT_KEY: &str = "agent_client_cert.key";
/// Agent 物料目录内记录签发时站点 CA 指纹的文件名（复用前校验 CA 未轮换）
pub const AGENT_CA_FINGERPRINT_FILE: &str = "agent_ca_fingerprint";

/// Agent 证书默认有效期（天，5 年）
const AGENT_CERT_VALIDITY_DAYS: i64 = 1825;

/// 证书 PEM 起始标记（复用校验的包含性检查）
const PEM_CERT_HEADER: &str = "-----BEGIN CERTIFICATE-----";

/// 证书物料就绪结果：QUIC 监听端文件路径 + 客户端证书 PEM 内容（供组包）。
#[derive(Debug, Clone)]
pub struct AgentCertMaterial {
    /// QUIC 监听端证书路径
    pub server_cert_path: String,
    /// QUIC 监听端私钥路径
    pub server_key_path: String,
    /// Agent 客户端证书 PEM（含单张证书）
    pub client_cert_pem: String,
    /// Agent 客户端私钥 PEM
    pub client_key_pem: String,
}

/// 枚举本机全部网卡的 IPv4/IPv6 地址（供 server 证书 SAN）。
///
/// 经 `if-addrs`（getifaddrs 的纯安全封装）真实枚举；枚举失败时仍保留
/// 固定回环项，保证 SAN 不为空。结果排序去重，保证同一机器多次签发
/// 产出的 SAN 序列稳定。
#[must_use]
pub fn collect_local_sans() -> Vec<IpAddr> {
    let ips: Vec<IpAddr> = if_addrs::get_if_addrs()
        .map(|ifaces| ifaces.into_iter().map(|iface| iface.ip()).collect())
        .unwrap_or_default();
    dedup_sort_ips(ips)
}

/// 排序去重：非回环地址升序在前，回环地址殿后（不新增条目，纯函数）。
fn sort_ips_loopback_last(mut ips: Vec<IpAddr>) -> Vec<IpAddr> {
    ips.sort_by_key(|ip| (ip.is_loopback(), *ip));
    ips.dedup();
    ips
}

/// 归并候选 IP 与固定回环项（127.0.0.1、::1 恒在且殿后），排序去重后返回
/// （纯函数，单测覆盖稳定性）。
fn dedup_sort_ips(ips: Vec<IpAddr>) -> Vec<IpAddr> {
    sort_ips_loopback_last(
        ips.into_iter()
            .chain([
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V6(Ipv6Addr::LOCALHOST),
            ])
            .collect(),
    )
}

/// server 证书 SAN 列表：本机 IP（去重排序，调用方传入的 collect_local_sans
/// 结果已含回环项）+ localhost 域名。
fn server_sans(ips: Vec<IpAddr>) -> Vec<SanType> {
    let mut sans: Vec<SanType> = sort_ips_loopback_last(ips)
        .into_iter()
        .map(SanType::IpAddress)
        .collect();
    if let Ok(dns) = "localhost".try_into() {
        sans.push(SanType::DnsName(dns));
    }
    sans
}

/// ensure_agent_certs 的同步核心：由站点 CA 一次性签发 server/client 证书，
/// 返回 (server 证书 PEM, server 私钥 PEM, client 证书 PEM, client 私钥 PEM)。
///
/// 供 `spawn_blocking` 调用，避免 rcgen 阻塞异步运行时。
fn issue_material(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    local_ips: Vec<IpAddr>,
) -> Result<(String, String, String, String), String> {
    let gen_error = |e: rcgen::Error| format!("rcgen 签发失败: {e}");

    let ca_key = KeyPair::from_pem(ca_key_pem).map_err(|e| format!("站点 CA 私钥解析失败: {e}"))?;
    let issuer = rcgen::Issuer::from_ca_cert_pem(ca_cert_pem, ca_key).map_err(gen_error)?;

    // server 证书：serverAuth EKU + 本机全部 IP 的 SAN（QUIC/HTTP3 服务器认证）
    let server_key = KeyPair::generate().map_err(gen_error)?;
    let mut server_params = CertificateParams::default();
    server_params.distinguished_name = leaf_dn("foims-agent-server");
    server_params.subject_alt_names = server_sans(local_ips);
    fill_leaf_extensions(&mut server_params, &[ExtendedKeyUsagePurpose::ServerAuth]);
    let server_cert = server_params
        .signed_by(&server_key, &issuer)
        .map_err(gen_error)?;

    // client 证书：clientAuth EKU（SAN 留空——客户端身份仅由证书链保证）
    let client_key = KeyPair::generate().map_err(gen_error)?;
    let mut client_params = CertificateParams::default();
    client_params.distinguished_name = leaf_dn("foims-agent-client");
    fill_leaf_extensions(&mut client_params, &[ExtendedKeyUsagePurpose::ClientAuth]);
    let client_cert = client_params
        .signed_by(&client_key, &issuer)
        .map_err(gen_error)?;

    Ok((
        server_cert.pem(),
        server_key.serialize_pem(),
        client_cert.pem(),
        client_key.serialize_pem(),
    ))
}

/// 叶子证书主题：仅 CN（组织信息沿用站点 CA，不在叶子重复）。
fn leaf_dn(common_name: &str) -> DistinguishedName {
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, common_name);
    dn
}

/// 叶子证书公共扩展：CA:FALSE、DigitalSignature KeyUsage、指定 EKU 与有效期。
fn fill_leaf_extensions(params: &mut CertificateParams, ekus: &[ExtendedKeyUsagePurpose]) {
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + time::Duration::days(AGENT_CERT_VALIDITY_DAYS);
    params.is_ca = IsCa::ExplicitNoCa;
    params.extended_key_usages = ekus.to_vec();
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
}

/// 为 agent 签发新 client 证书（续期用）：leaf_dn("foims-agent-client") +
/// clientAuth EKU，与 [`issue_material`] 的 client 证书同构。
/// 返回 (证书 PEM, 私钥 PEM, 失效时刻 UNIX 秒)；供 `spawn_blocking` 调用。
fn issue_client_material(
    ca_cert_pem: &str,
    ca_key_pem: &str,
) -> Result<(String, String, i64), String> {
    let gen_error = |e: rcgen::Error| format!("rcgen 签发失败: {e}");

    let ca_key = KeyPair::from_pem(ca_key_pem).map_err(|e| format!("站点 CA 私钥解析失败: {e}"))?;
    let issuer = rcgen::Issuer::from_ca_cert_pem(ca_cert_pem, ca_key).map_err(gen_error)?;

    // client 证书：clientAuth EKU（SAN 留空——客户端身份仅由证书链保证）
    let client_key = KeyPair::generate().map_err(gen_error)?;
    let mut client_params = CertificateParams::default();
    client_params.distinguished_name = leaf_dn("foims-agent-client");
    fill_leaf_extensions(&mut client_params, &[ExtendedKeyUsagePurpose::ClientAuth]);
    let not_after_unix = client_params.not_after.unix_timestamp();
    let client_cert = client_params
        .signed_by(&client_key, &issuer)
        .map_err(gen_error)?;

    Ok((
        client_cert.pem(),
        client_key.serialize_pem(),
        not_after_unix,
    ))
}

/// 读取 PEM 文本，不存在/不可读时返回 Err（错误信息含路径）。
async fn read_pem(path: &Path, label: &str) -> Result<String, String> {
    tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("读取{label}失败 {}: {e}", path.display()))
}

/// 写入证书（公开物料，默认 0644）。
async fn write_cert_file(path: &PathBuf, content: &str) -> Result<(), String> {
    tokio::fs::write(path, content)
        .await
        .map_err(|e| format!("写入证书失败 {}: {e}", path.display()))
}

/// 写入私钥（unix 下 0600 独占创建，消除"先写后 chmod"窗口期）。
async fn write_key_file(path: &PathBuf, content: &str) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    #[cfg(unix)]
    {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .await
            .map_err(|e| format!("写入私钥失败 {}: {e}", path.display()))?;
        file.write_all(content.as_bytes())
            .await
            .map_err(|e| format!("写入私钥失败 {}: {e}", path.display()))?;
        file.flush()
            .await
            .map_err(|e| format!("刷新私钥失败 {}: {e}", path.display()))
    }

    #[cfg(not(unix))]
    {
        tokio::fs::write(path, content)
            .await
            .map_err(|e| format!("写入私钥失败 {}: {e}", path.display()))
    }
}

/// 计算证书 PEM 的 SHA-256 指纹（DER 编码，十六进制小写文本）。
///
/// 仅接受证书段 PEM；解析失败或不含证书时报错（复用校验据此判定不可信）。
fn ca_cert_fingerprint(ca_cert_pem: &str) -> Result<String, String> {
    let parsed = pem::parse(ca_cert_pem).map_err(|e| format!("解析站点 CA 证书 PEM 失败: {e}"))?;
    if parsed.tag() != "CERTIFICATE" {
        return Err("站点 CA 文件不含证书段".to_string());
    }
    let digest = Sha256::digest(parsed.contents());
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// 比对存量指纹文件内容与当前 CA 指纹（trim 后忽略大小写，兼容手工维护的大写十六进制）。
fn fingerprint_matches(current: &str, stored: &str) -> bool {
    stored.trim().eq_ignore_ascii_case(current)
}

/// 复用判定：四个物料文件可读 + 指纹文件与当前 CA 指纹一致 + 两张证书 PEM
/// 含证书起始头，全部满足才返回既有物料；任一不满足返回 None（走重签路径）。
async fn try_reuse_material(dir: &Path, ca_fingerprint: &str) -> Option<AgentCertMaterial> {
    let server_cert_path = dir.join(SERVER_CERT);
    let server_key_path = dir.join(SERVER_KEY);
    let client_cert_path = dir.join(CLIENT_CERT);
    let client_key_path = dir.join(CLIENT_KEY);

    // 任一物料不可读：按缺失处理（首次启动为常态，无需告警）；
    // server 私钥仅校验可读性（内容格式由 TLS 加载兜底），其余内容随结果返回
    let Ok(server_cert) = read_pem(&server_cert_path, "server 证书").await else {
        return None;
    };
    if read_pem(&server_key_path, "server 私钥").await.is_err() {
        return None;
    }
    let Ok(client_cert) = read_pem(&client_cert_path, "client 证书").await else {
        return None;
    };
    let Ok(client_key) = read_pem(&client_key_path, "client 私钥").await else {
        return None;
    };

    // 站点 CA 指纹校验：文件缺失或不匹配均重签，防止 CA 轮换后旧物料被复用
    match tokio::fs::read_to_string(dir.join(AGENT_CA_FINGERPRINT_FILE)).await {
        Ok(stored) if fingerprint_matches(ca_fingerprint, &stored) => {}
        Ok(_) => {
            foims_common::log_warn!(
                "log.agent.certs_reuse_rejected",
                detail = "ca_fingerprint_mismatch"
            );
            return None;
        }
        Err(_) => {
            foims_common::log_warn!(
                "log.agent.certs_reuse_rejected",
                detail = "ca_fingerprint_missing"
            );
            return None;
        }
    }

    // 证书内容最低限度校验：必须含证书起始头（私钥格式由 TLS 加载兜底）
    if !server_cert.contains(PEM_CERT_HEADER) || !client_cert.contains(PEM_CERT_HEADER) {
        foims_common::log_warn!(
            "log.agent.certs_reuse_rejected",
            detail = "cert_pem_header_missing"
        );
        return None;
    }

    Some(AgentCertMaterial {
        server_cert_path: server_cert_path.display().to_string(),
        server_key_path: server_key_path.display().to_string(),
        client_cert_pem: client_cert,
        client_key_pem: client_key,
    })
}

/// 证书签发/续期进程内互斥：client 证书重签与磁盘替换是多步操作，并发续期
/// 会交错写盘导致证书与私钥混搭不一致；同一时刻仅允许一次签发。
static ISSUE_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// 确保四份证书物料齐备并返回：物料齐备且与当前站点 CA 指纹一致时直接复用；
/// 否则要求站点 CA 存在，经 rcgen 签发 server（SAN=本机全部 IP + localhost）
/// 与 client 证书，连同 CA 指纹文件一并落盘。
///
/// 返回 Err 时调用方不应启动 QUIC 监听（cert 未就绪则 mTLS 无法建立）。
pub async fn ensure_agent_certs() -> Result<AgentCertMaterial, String> {
    // 与续期共用互斥：启动签发不会与运行期续期交错写盘
    let _guard = ISSUE_LOCK.lock().await;
    ensure_agent_certs_in(
        Path::new(AGENT_CERT_DIR),
        Path::new(super::SITE_CA_PATH),
        Path::new(super::SITE_CA_KEY_PATH),
    )
    .await
}

/// [`ensure_agent_certs`] 的可测核心：物料目录与 CA 路径由参数注入
/// （单测以临时目录替换，避免触碰 /etc/ssl 生产路径）。
async fn ensure_agent_certs_in(
    dir: &Path,
    ca_cert_path: &Path,
    ca_key_path: &Path,
) -> Result<AgentCertMaterial, String> {
    // 站点 CA 证书两条路径都需要：复用时校验指纹，重签时作为签发者
    let ca_cert = read_pem(ca_cert_path, "站点 CA 证书").await?;
    let ca_fingerprint = ca_cert_fingerprint(&ca_cert)?;

    // 四文件齐备且指纹匹配：直接复用既有物料（证书过期由 TLS 握手暴露，CA 轮换由指纹校验兜底）
    if let Some(material) = try_reuse_material(dir, &ca_fingerprint).await {
        return Ok(material);
    }

    // 重签路径：要求站点 CA 私钥存在（证书管理页生成）
    let ca_key = read_pem(ca_key_path, "站点 CA 私钥").await?;

    // rcgen 签发移出异步线程
    let (server_pem, server_key_pem, client_pem, client_key_pem) =
        tokio::task::spawn_blocking(move || {
            issue_material(&ca_cert, &ca_key, collect_local_sans())
        })
        .await
        .map_err(|e| format!("签发任务调度失败: {e}"))??;

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("创建证书目录 {} 失败: {e}", dir.display()))?;
    write_cert_file(&dir.join(SERVER_CERT), &server_pem).await?;
    write_key_file(&dir.join(SERVER_KEY), &server_key_pem).await?;
    write_cert_file(&dir.join(CLIENT_CERT), &client_pem).await?;
    write_key_file(&dir.join(CLIENT_KEY), &client_key_pem).await?;
    // 记录本次签发所用 CA 指纹，供下次启动复用校验（公开物料，默认 0644）
    write_cert_file(&dir.join(AGENT_CA_FINGERPRINT_FILE), &ca_fingerprint).await?;

    foims_common::log_info!("log.agent.certs_issued", dir = AGENT_CERT_DIR);
    Ok(AgentCertMaterial {
        server_cert_path: dir.join(SERVER_CERT).display().to_string(),
        server_key_path: dir.join(SERVER_KEY).display().to_string(),
        client_cert_pem: client_pem,
        client_key_pem,
    })
}

/// 续期 agent client 证书：以站点 CA 重签同身份（CN=foims-agent-client、
/// clientAuth EKU）新证书，并同步替换磁盘上的 CLIENT_CERT/CLIENT_KEY
/// （后续下载组包即携带新证书）。返回 (新证书 PEM, 新私钥 PEM, 失效时刻
/// RFC 3339)；QUIC 监听端校验只锚定 CA，替换 client 物料无需重启监听。
pub async fn renew_client_cert() -> Result<(String, String, String), String> {
    // try_lock 互斥：并发续期直接返回明确错误而非排队重复签发
    let _guard = ISSUE_LOCK.try_lock().map_err(|_| {
        foims_common::log_warn!("log.agent.renew_conflict");
        "已有证书续期正在进行，请稍后重试".to_string()
    })?;
    renew_client_cert_in(
        Path::new(AGENT_CERT_DIR),
        Path::new(super::SITE_CA_PATH),
        Path::new(super::SITE_CA_KEY_PATH),
    )
    .await
}

/// [`renew_client_cert`] 的可测核心：目录与 CA 路径参数注入。
async fn renew_client_cert_in(
    dir: &Path,
    ca_cert_path: &Path,
    ca_key_path: &Path,
) -> Result<(String, String, String), String> {
    let ca_cert = read_pem(ca_cert_path, "站点 CA 证书").await?;
    let ca_key = read_pem(ca_key_path, "站点 CA 私钥").await?;

    // rcgen 签发移出异步线程
    let (cert_pem, key_pem, not_after_unix) =
        tokio::task::spawn_blocking(move || issue_client_material(&ca_cert, &ca_key))
            .await
            .map_err(|e| format!("签发任务调度失败: {e}"))??;
    let not_after = chrono::DateTime::from_timestamp(not_after_unix, 0)
        .ok_or_else(|| format!("证书失效时刻非法: {not_after_unix}"))?
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("创建证书目录 {} 失败: {e}", dir.display()))?;
    write_cert_file(&dir.join(CLIENT_CERT), &cert_pem).await?;
    write_key_file(&dir.join(CLIENT_KEY), &key_pem).await?;

    foims_common::log_info!("log.agent.client_cert_renewed", not_after = not_after);
    Ok((cert_pem, key_pem, not_after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 解析测试用 IP 字面量（测试代码允许 unwrap）
    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn ip候选_去重排序稳定() {
        // 重复与乱序输入：排序去重后含固定回环项，重复出现仅一次
        let merged = dedup_sort_ips(vec![
            ip("::1"),
            ip("192.168.1.10"),
            ip("10.0.0.2"),
            ip("192.168.1.10"),
        ]);
        assert_eq!(
            merged,
            vec![
                ip("10.0.0.2"),
                ip("192.168.1.10"),
                ip("127.0.0.1"),
                ip("::1")
            ],
            "应排序去重且回环项恒在"
        );

        // 相同输入二次调用结果一致（排序稳定性）
        let again = dedup_sort_ips(vec![
            ip("::1"),
            ip("192.168.1.10"),
            ip("10.0.0.2"),
            ip("192.168.1.10"),
        ]);
        assert_eq!(merged, again);

        // 空输入：仅固定回环项
        assert_eq!(dedup_sort_ips(Vec::new()), vec![ip("127.0.0.1"), ip("::1")]);
    }

    #[test]
    fn server_sans_含本机ip与localhost域名() {
        let sans = server_sans(vec![ip("10.0.0.5"), ip("127.0.0.1")]);
        let has_dns = sans
            .iter()
            .any(|s| matches!(s, SanType::DnsName(d) if d.as_str() == "localhost"));
        assert!(has_dns, "localhost 域名 SAN 应存在");
        assert_eq!(sans.len(), 3, "去重后应仅 2 个 IP + 1 个域名: {sans:?}");
    }

    // ==================== CA 指纹复用校验 ====================

    /// 构造一次性临时目录（无 tempfile 依赖，沿用 foims-services 先例）。
    fn temp_dir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "foims-agent-cert-test-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).ok();
        base
    }

    /// 生成测试用自签 CA 物料（证书 PEM, 私钥 PEM）。
    fn test_ca(common_name: &str) -> (String, String) {
        let ca_key = KeyPair::generate().unwrap_or_else(|e| panic!("生成 CA 密钥失败: {e}"));
        let mut ca_params = CertificateParams::default();
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CommonName, common_name);
        ca_params.distinguished_name = ca_dn;
        ca_params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca_cert = ca_params
            .self_signed(&ca_key)
            .unwrap_or_else(|e| panic!("生成 CA 失败: {e}"));
        (ca_cert.pem(), ca_key.serialize_pem())
    }

    /// 将 CA 物料写入临时目录，返回 (证书路径, 私钥路径)。
    fn write_ca(dir: &Path, ca: &(String, String)) -> (PathBuf, PathBuf) {
        let cert_path = dir.join("ca.pem");
        let key_path = dir.join("ca.key");
        fs::write(&cert_path, &ca.0).unwrap_or_else(|e| panic!("写 CA 证书失败: {e}"));
        fs::write(&key_path, &ca.1).unwrap_or_else(|e| panic!("写 CA 私钥失败: {e}"));
        (cert_path, key_path)
    }

    #[tokio::test]
    async fn ca指纹匹配_应复用既有物料() {
        let material_dir = temp_dir("reuse-mat");
        let ca_dir = temp_dir("reuse-ca");
        let ca = test_ca("FOIMS Test CA Reuse");
        let (ca_cert_path, ca_key_path) = write_ca(&ca_dir, &ca);

        // 首次调用走重签路径：落盘的指纹文件应等于当前 CA 指纹
        let first = ensure_agent_certs_in(&material_dir, &ca_cert_path, &ca_key_path)
            .await
            .unwrap();
        let stored = fs::read_to_string(material_dir.join(AGENT_CA_FINGERPRINT_FILE)).unwrap();
        assert_eq!(
            stored.trim(),
            ca_cert_fingerprint(&ca.0).unwrap(),
            "指纹文件应记录当前站点 CA 指纹"
        );

        // 二次调用指纹匹配：应复用同一份物料（client 证书逐字节一致）
        let second = ensure_agent_certs_in(&material_dir, &ca_cert_path, &ca_key_path)
            .await
            .unwrap();
        assert_eq!(
            first.client_cert_pem, second.client_cert_pem,
            "指纹匹配应复用既有物料而非重签"
        );

        let _ = fs::remove_dir_all(&material_dir);
        let _ = fs::remove_dir_all(&ca_dir);
    }

    #[tokio::test]
    async fn ca指纹不匹配_应重签并更新指纹文件() {
        let material_dir = temp_dir("mismatch-mat");
        let ca_dir = temp_dir("mismatch-ca");
        let ca_old = test_ca("FOIMS Test CA Old");
        let (ca_cert_path, ca_key_path) = write_ca(&ca_dir, &ca_old);
        let first = ensure_agent_certs_in(&material_dir, &ca_cert_path, &ca_key_path)
            .await
            .unwrap();

        // 模拟站点 CA 轮换：替换 CA 后指纹不匹配，应重签全部物料
        let ca_new = test_ca("FOIMS Test CA New");
        write_ca(&ca_dir, &ca_new);
        let second = ensure_agent_certs_in(&material_dir, &ca_cert_path, &ca_key_path)
            .await
            .unwrap();
        assert_ne!(
            first.client_cert_pem, second.client_cert_pem,
            "CA 轮换后应重新签发物料"
        );
        let stored = fs::read_to_string(material_dir.join(AGENT_CA_FINGERPRINT_FILE)).unwrap();
        assert_eq!(
            stored.trim(),
            ca_cert_fingerprint(&ca_new.0).unwrap(),
            "重签后指纹文件应更新为新 CA 指纹"
        );

        let _ = fs::remove_dir_all(&material_dir);
        let _ = fs::remove_dir_all(&ca_dir);
    }

    #[tokio::test]
    async fn 指纹文件缺失_应重签并补写指纹文件() {
        let material_dir = temp_dir("missing-mat");
        let ca_dir = temp_dir("missing-ca");
        let ca = test_ca("FOIMS Test CA Missing");
        let (ca_cert_path, ca_key_path) = write_ca(&ca_dir, &ca);
        let first = ensure_agent_certs_in(&material_dir, &ca_cert_path, &ca_key_path)
            .await
            .unwrap();

        // 旧版本升级场景：物料齐备但无指纹文件，应重签兜底
        fs::remove_file(material_dir.join(AGENT_CA_FINGERPRINT_FILE)).unwrap();
        let second = ensure_agent_certs_in(&material_dir, &ca_cert_path, &ca_key_path)
            .await
            .unwrap();
        assert_ne!(
            first.client_cert_pem, second.client_cert_pem,
            "指纹文件缺失应走重签路径"
        );
        assert!(
            material_dir.join(AGENT_CA_FINGERPRINT_FILE).exists(),
            "重签后应补写指纹文件"
        );

        let _ = fs::remove_dir_all(&material_dir);
        let _ = fs::remove_dir_all(&ca_dir);
    }

    #[test]
    fn ca指纹_为确定性小写hex且拒绝非证书输入() {
        let ca = test_ca("FOIMS Test CA Fingerprint");
        let fp = ca_cert_fingerprint(&ca.0).unwrap();
        assert_eq!(fp.len(), 64, "SHA-256 指纹应为 64 个十六进制字符: {fp}");
        assert!(
            fp.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "指纹应为小写十六进制: {fp}"
        );
        assert_eq!(
            fp,
            ca_cert_fingerprint(&ca.0).unwrap(),
            "同一证书的指纹应确定"
        );
        assert!(
            ca_cert_fingerprint("not a pem").is_err(),
            "非 PEM 输入应报错"
        );
    }

    #[test]
    fn 指纹比对_trim后忽略大小写() {
        assert!(fingerprint_matches("abc", "abc\n"), "尾部换行应被 trim");
        assert!(
            fingerprint_matches("abc", " ABC "),
            "大写十六进制应视为一致"
        );
        assert!(!fingerprint_matches("abc", "abd"), "内容不同应判不匹配");
        assert!(!fingerprint_matches("abc", ""), "空内容应判不匹配");
    }

    #[tokio::test]
    async fn 续期client证书_应重签并替换磁盘物料() {
        let material_dir = temp_dir("renew-mat");
        let ca_dir = temp_dir("renew-ca");
        let ca = test_ca("FOIMS Test CA Renew");
        let (ca_cert_path, ca_key_path) = write_ca(&ca_dir, &ca);
        let initial = ensure_agent_certs_in(&material_dir, &ca_cert_path, &ca_key_path)
            .await
            .unwrap();

        // 续期：应重签出与初始不同的新证书并替换磁盘 CLIENT_CERT/CLIENT_KEY
        let (cert_pem, key_pem, not_after) =
            renew_client_cert_in(&material_dir, &ca_cert_path, &ca_key_path)
                .await
                .unwrap();
        assert_ne!(
            cert_pem, initial.client_cert_pem,
            "续期应重签新证书而非复用"
        );
        assert!(cert_pem.contains(PEM_CERT_HEADER), "证书 PEM 应含证书段");
        assert!(key_pem.contains("BEGIN PRIVATE KEY"), "私钥应为 PKCS#8 PEM");
        let parsed = chrono::DateTime::parse_from_rfc3339(&not_after)
            .unwrap_or_else(|e| panic!("not_after 应为合法 RFC3339: {e}"))
            .with_timezone(&chrono::Utc);
        let days = (parsed - chrono::Utc::now()).num_days();
        assert!(
            (1820..=1825).contains(&days),
            "新证书有效期应约 5 年: {days}"
        );
        let disk_cert = fs::read_to_string(material_dir.join(CLIENT_CERT))
            .unwrap_or_else(|e| panic!("读取续期后证书失败: {e}"));
        assert_eq!(disk_cert, cert_pem, "磁盘 client 证书应已替换为续期产物");

        let _ = fs::remove_dir_all(&material_dir);
        let _ = fs::remove_dir_all(&ca_dir);
    }
}
