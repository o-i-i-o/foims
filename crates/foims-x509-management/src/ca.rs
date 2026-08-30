//! 站点根 CA 与导入 CA 池管理：生成、导入、状态查询与导出。
//!
//! CA 存储分两处：
//! - 站点根 CA：`/etc/ssl/foims-ca/`（ca.pem + 可选 ca.key），由"生成CA"写入，
//!   是程序 HTTPS 与终端信任的默认签发 CA；
//! - 导入 CA 池：`/etc/ssl/foims-import-cas/{id}/`（ca.pem + ca.key），
//!   由"导入CA"写入，仅作为证书签发的备选 CA，不改变站点根 CA。
//!
//! 证书生成强制由 CA 签发（根 CA 或导入 CA 池中任选，无 CA 时拒绝生成），
//! 不再提供自签名回退。
//!
//! CA 证书本身是公开数据，私钥绝不允许通过任何接口外发。

use std::path::{Path, PathBuf};

use foims_common::msg;
use rcgen::{BasicConstraints, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose};
use serde::{Deserialize, Serialize};

use crate::error::CertManagerError;
use crate::generate::write_key_file;

/// 站点 CA 存储目录
pub const CA_DIR: &str = "/etc/ssl/foims-ca";
/// 导入 CA 池目录（每个 CA 独立子目录：ca.pem + ca.key）
pub const IMPORT_CA_DIR: &str = "/etc/ssl/foims-import-cas";
/// 站点根 CA 在 CA 列表中的固定标识
pub const ROOT_CA_ID: &str = "root";

const CA_CERT_FILE: &str = "ca.pem";
const CA_KEY_FILE: &str = "ca.key";

const DEFAULT_CA_VALIDITY_DAYS: i64 = 7300;
const MAX_CA_VALIDITY_DAYS: i64 = 36500;

/// 导入 CA 目录名冲突重试上限（毫秒时间戳下冲突概率极低，仅作有界保护）
const MAX_DIR_NAME_RETRIES: u64 = 100;

#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateCaRequest {
    pub common_name: String,
    pub organization: Option<String>,
    pub organizational_unit: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub locality: Option<String>,
    /// 有效期（天），缺省 7300（约 20 年）
    pub validity_days: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CaStatus {
    pub available: bool,
    /// 私钥是否在本机（决定能否签发证书）
    pub has_key: bool,
    pub subject_cn: Option<String>,
    pub issuer_cn: Option<String>,
    pub not_before: Option<chrono::DateTime<chrono::Utc>>,
    pub not_after: Option<chrono::DateTime<chrono::Utc>>,
    pub days_remaining: Option<i64>,
}

/// CA 列表项（证书生成弹窗的 CA 下拉框数据源）
#[derive(Debug, Serialize, Deserialize)]
pub struct CaInfo {
    /// CA 标识：根 CA 固定为 "root"，导入 CA 为其目录名
    pub id: String,
    /// CA 主题 CN
    pub name: Option<String>,
    /// 来源："root"（自生成根 CA）/ "imported"（导入 CA）
    pub source: String,
    /// 私钥是否在本机（无私钥的 CA 不能签发）
    pub has_key: bool,
    pub not_before: Option<chrono::DateTime<chrono::Utc>>,
    pub not_after: Option<chrono::DateTime<chrono::Utc>>,
    pub days_remaining: Option<i64>,
}

fn ca_cert_path() -> PathBuf {
    Path::new(CA_DIR).join(CA_CERT_FILE)
}

/// CA 证书原子写入：先写同目录临时文件（create_new 独占 + fsync），
/// 再 rename 覆盖目标文件。
///
/// ca.pem 在程序运行期被签发路径与 list_cas 清点并发读取，直接
/// 截断写（tokio::fs::write）会让读者拿到半截 PEM；临时文件 + rename
/// 保证任意时刻读到的都是完整的旧版或新版内容（与 foims-init
/// config.rs 的原子写法同口径）。私钥不走此路径——write_key_file
/// 已有 create_new(0600) 独占语义。
async fn write_ca_cert_atomic(target: &Path, cert_pem: &[u8]) -> Result<(), CertManagerError> {
    use tokio::io::AsyncWriteExt;

    let write_err = |e: std::io::Error| {
        CertManagerError::Internal(msg("server.certificate.write_failed").with("error", e))
    };

    let mut seq: u64 = 0;
    loop {
        // 临时文件名 = 目标名 + 进程号 + 微秒时间戳 + 序号（create_new 独占）
        let stamp = chrono::Utc::now().timestamp_micros();
        let tmp_path = PathBuf::from(format!(
            "{}.{}.{}.{}.tmp",
            target.display(),
            std::process::id(),
            stamp,
            seq
        ));
        let mut file = match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .await
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                seq += 1;
                if seq > MAX_DIR_NAME_RETRIES {
                    return Err(write_err(e));
                }
                continue;
            }
            Err(e) => return Err(write_err(e)),
        };

        // 写入并 fsync 后 rename；写失败与 rename 失败同样清理临时文件
        let write_result: std::io::Result<()> = async {
            file.write_all(cert_pem).await?;
            file.sync_all().await
        }
        .await;
        if let Err(e) = write_result {
            if let Err(remove_err) = tokio::fs::remove_file(&tmp_path).await {
                foims_common::log_warn!("log.certificate.ca_tmp_remove_failed", error = remove_err);
            }
            return Err(write_err(e));
        }

        // 临时文件继承目标既有权限（rename 替换后保留原访问控制，尽力而为）
        if let Ok(meta) = tokio::fs::metadata(target).await
            && let Err(e) = tokio::fs::set_permissions(&tmp_path, meta.permissions()).await
        {
            foims_common::log_warn!("log.certificate.ca_tmp_chmod_failed", error = e);
        }

        if let Err(e) = tokio::fs::rename(&tmp_path, target).await {
            if let Err(remove_err) = tokio::fs::remove_file(&tmp_path).await {
                foims_common::log_warn!("log.certificate.ca_tmp_remove_failed", error = remove_err);
            }
            return Err(write_err(e));
        }
        return Ok(());
    }
}

fn ca_key_path() -> PathBuf {
    Path::new(CA_DIR).join(CA_KEY_FILE)
}

fn import_ca_dir(id: &str) -> PathBuf {
    Path::new(IMPORT_CA_DIR).join(id)
}

/// CA 标识只允许出现字母数字与连字符/下划线，防止路径穿越
fn valid_ca_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// 读取 CA 证书 PEM；不存在时返回 None
async fn read_ca_cert() -> Option<Vec<u8>> {
    tokio::fs::read(ca_cert_path()).await.ok()
}

/// 校验证书是 CA（BasicConstraints CA:TRUE）、处于有效期内；
/// 提供私钥时一并校验与证书公钥匹配（防止导入错配的证书/私钥对）
fn validate_ca(cert_pem: &[u8], key_pem: Option<&str>) -> Result<(), CertManagerError> {
    let invalid = || CertManagerError::Validation(msg("server.certificate.ca_invalid"));
    let unparsable = || CertManagerError::Validation(msg("server.certificate.cert_file_invalid"));

    let pems = pem::parse_many(cert_pem).map_err(|_| unparsable())?;
    let Some(block) = pems.iter().find(|p| p.tag() == "CERTIFICATE") else {
        return Err(unparsable());
    };
    let (_, cert) =
        x509_parser::parse_x509_certificate(block.contents()).map_err(|_| unparsable())?;

    let is_ca = cert
        .basic_constraints()
        .map_err(|_| invalid())?
        .is_some_and(|ext| ext.value.ca);
    if !is_ca {
        return Err(invalid());
    }

    // 有效期校验：当前时间不在 [not_before, not_after] 内的 CA 视为无效
    // （过期 CA 导入后签出的全部叶子证书都不可信）
    let now = chrono::Utc::now().timestamp();
    let validity = &cert.tbs_certificate.validity;
    if validity.not_before.timestamp() > now || validity.not_after.timestamp() < now {
        return Err(invalid());
    }

    if let Some(key_pem) = key_pem {
        ensure_key_matches(&cert, key_pem)?;
    }
    Ok(())
}

/// 校验私钥可解析且其公钥与证书公钥匹配（CA 与叶子证书导入共用）
pub(crate) fn ensure_key_matches(
    cert: &x509_parser::certificate::X509Certificate<'_>,
    key_pem: &str,
) -> Result<(), CertManagerError> {
    let invalid = || CertManagerError::Validation(msg("server.certificate.ca_invalid"));
    let key_pair = KeyPair::from_pem(key_pem).map_err(|_| invalid())?;
    let cert_key: &[u8] = cert
        .tbs_certificate
        .subject_pki
        .subject_public_key
        .data
        .as_ref();
    if cert_key != key_pair.public_key_raw() {
        return Err(invalid());
    }
    Ok(())
}

/// 校验证书/私钥对：两者均可解析且公钥匹配（导入叶子证书用，不要求 CA 属性）。
/// 私钥为 "Hello" 之类的假内容在此即被拒绝。
pub(crate) fn validate_cert_key_pair(
    cert_pem: &[u8],
    key_pem: &str,
) -> Result<(), CertManagerError> {
    let unparsable = || CertManagerError::Validation(msg("server.certificate.cert_file_invalid"));
    let pems = pem::parse_many(cert_pem).map_err(|_| unparsable())?;
    let Some(block) = pems.iter().find(|p| p.tag() == "CERTIFICATE") else {
        return Err(unparsable());
    };
    let (_, cert) =
        x509_parser::parse_x509_certificate(block.contents()).map_err(|_| unparsable())?;
    // 错配（含私钥不可解析）按无效证书/私钥口径拒绝
    ensure_key_matches(&cert, key_pem)
        .map_err(|_| CertManagerError::Validation(msg("server.certificate.cert_file_invalid")))
}

/// 请求参数校验（等价 Validate 派生：本 crate 未依赖 validator，手写实现）：
/// common_name trim 后 1..=64 字符、organization/organizational_unit/state/
/// locality ≤64 字符、country trim 后为 2 字符。
fn validate_generate_ca_request(req: &GenerateCaRequest) -> Result<(), CertManagerError> {
    validate_dn_fields(
        &req.common_name,
        req.organization.as_deref(),
        req.organizational_unit.as_deref(),
        req.country.as_deref(),
        req.state.as_deref(),
        req.locality.as_deref(),
    )
}

/// DN 字段统一校验（CA 与叶子证书共用同一套口径）：
/// CN trim 后 1..=64、org/ou/state/locality ≤64、country trim 后必须为 2 字符。
pub(crate) fn validate_dn_fields(
    common_name: &str,
    organization: Option<&str>,
    organizational_unit: Option<&str>,
    country: Option<&str>,
    state: Option<&str>,
    locality: Option<&str>,
) -> Result<(), CertManagerError> {
    let validation = |key: &'static str| CertManagerError::Validation(msg(key));

    let cn_len = common_name.trim().chars().count();
    if cn_len == 0 {
        return Err(validation("server.certificate.common_name_required"));
    }
    if cn_len > 64 {
        return Err(validation("server.certificate.common_name_length"));
    }
    if let Some(org) = organization
        && org.trim().chars().count() > 64
    {
        return Err(validation("server.certificate.organization_length"));
    }
    if let Some(ou) = organizational_unit
        && ou.trim().chars().count() > 64
    {
        return Err(validation("server.certificate.organizational_unit_length"));
    }
    if let Some(state) = state
        && state.trim().chars().count() > 64
    {
        return Err(validation("server.certificate.state_length"));
    }
    if let Some(locality) = locality
        && locality.trim().chars().count() > 64
    {
        return Err(validation("server.certificate.locality_length"));
    }
    if let Some(country) = country
        && country.trim().len() != 2
    {
        return Err(validation("server.certificate.country_invalid"));
    }
    Ok(())
}

/// 组装 DN（与叶子证书共用同一套字段约定）
fn build_dn(req: &GenerateCaRequest) -> DistinguishedName {
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, req.common_name.trim());
    if let Some(org) = req
        .organization
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::OrganizationName, org);
    }
    if let Some(ou) = req
        .organizational_unit
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::OrganizationalUnitName, ou);
    }
    if let Some(country) = req
        .country
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::CountryName, country);
    }
    if let Some(state) = req
        .state
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::StateOrProvinceName, state);
    }
    if let Some(locality) = req
        .locality
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        dn.push(DnType::LocalityName, locality);
    }
    dn
}

/// 生成自签名根 CA 并写入 CA 目录，返回证书路径。
///
/// 覆盖保护：ca.pem 或 ca.key 任一已存在即返回 Conflict（站点根 CA 重建属高危
/// 操作，会使既有叶子证书与 HTTPS 信任全部失效，需先删除旧 CA 再生成）。
/// x509v3 扩展：BasicConstraints CA:TRUE（无路径长度限制）、
/// KeyUsage 含 keyCertSign/cRLSign，不含 EKU（CA 不做终端认证）。
pub async fn generate_ca(req: GenerateCaRequest) -> Result<PathBuf, CertManagerError> {
    // 统一请求校验（CN/组织/国家，country 先 trim 再判长度，与叶子证书同口径）
    validate_generate_ca_request(&req)?;

    // 覆盖保护（先于任何写入）：任一文件已存在即拒绝，不提供静默覆盖
    let cert_exists = tokio::fs::try_exists(ca_cert_path()).await.unwrap_or(false);
    let key_exists = tokio::fs::try_exists(ca_key_path()).await.unwrap_or(false);
    if cert_exists || key_exists {
        return Err(CertManagerError::Conflict(msg(
            "server.certificate.ca_already_exists",
        )));
    }

    let validity_days = req
        .validity_days
        .map_or(DEFAULT_CA_VALIDITY_DAYS, i64::from)
        .clamp(1, MAX_CA_VALIDITY_DAYS);

    tokio::fs::create_dir_all(CA_DIR).await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        )
    })?;

    let generation = tokio::task::spawn_blocking(move || {
        let key_pair = KeyPair::generate().map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.generate_failed").with("error", e.to_string()),
            )
        })?;

        let now = time::OffsetDateTime::now_utc();
        let mut params = rcgen::CertificateParams::default();
        params.not_before = now;
        params.not_after = now + time::Duration::days(validity_days);
        params.distinguished_name = build_dn(&req);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        // 根 CA 的密钥用途：签发证书与吊销列表（RFC 5280 要求 keyCertSign）
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];

        let cert = params.self_signed(&key_pair).map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.generate_failed").with("error", e.to_string()),
            )
        })?;

        Ok::<_, CertManagerError>((cert.pem(), key_pair.serialize_pem()))
    })
    .await
    .map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.generate_failed").with("error", e.to_string()),
        )
    })??;

    // 先写证书后写私钥：证书写失败时目录仍为空（一致性最好）；
    // 私钥写失败至多留下"仅证书不可签发"的一致状态（cert-only 语义），
    // 不会出现"新 key + 旧 cert"的错配；证书走临时文件 + rename 原子写
    write_ca_cert_atomic(&ca_cert_path(), generation.0.as_bytes()).await?;
    write_key_file(&ca_key_path(), &generation.1).await?;

    Ok(ca_cert_path())
}

/// 导入已有 CA（证书 + 私钥），写入导入 CA 池的独立子目录，返回该目录。
///
/// 校验：证书可解析、BasicConstraints CA:TRUE、私钥可解析且与证书公钥匹配。
/// 导入 CA 不覆盖站点根 CA；目录以 `import_{毫秒时间戳}` 命名，并用
/// `create_dir` 原子独占创建（已存在即失败），冲突时按序号重试（有界），
/// 消除"预检-创建"之间的 check-then-act 竞态窗口。
pub async fn import_ca(cert_pem: Vec<u8>, key_pem: Vec<u8>) -> Result<PathBuf, CertManagerError> {
    if cert_pem.is_empty() || key_pem.is_empty() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.ca_key_required",
        )));
    }
    let key_pem = String::from_utf8_lossy(&key_pem).to_string();
    // 解析与匹配校验移入阻塞线程（KeyPair::from_pem 涉及密钥计算）
    let validated = {
        let cert_for_check = cert_pem.clone();
        let key_for_check = key_pem.clone();
        tokio::task::spawn_blocking(move || validate_ca(&cert_for_check, Some(&key_for_check)))
            .await
            .map_err(|e| {
                CertManagerError::Internal(
                    msg("server.certificate.generate_failed").with("error", e.to_string()),
                )
            })?
    };
    validated?;

    tokio::fs::create_dir_all(IMPORT_CA_DIR)
        .await
        .map_err(|e| {
            CertManagerError::Internal(
                msg("server.certificate.write_failed").with("error", e.to_string()),
            )
        })?;

    // 目录名：import_{毫秒时间戳}，create_dir 独占创建，冲突时递增序号重试，
    // 保证并发/双击导入互不覆盖、也不交错写同一目录
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut seq: u64 = 0;
    let target_dir = loop {
        let id = match seq {
            0 => format!("import_{timestamp}"),
            n => format!("import_{timestamp}_{n}"),
        };
        match tokio::fs::create_dir(import_ca_dir(&id)).await {
            Ok(()) => break import_ca_dir(&id),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                seq += 1;
                if seq > MAX_DIR_NAME_RETRIES {
                    return Err(CertManagerError::Internal(
                        msg("server.certificate.write_failed")
                            .with("error", format!("目录名冲突重试耗尽: import_{timestamp}")),
                    ));
                }
            }
            Err(e) => {
                return Err(CertManagerError::Internal(
                    msg("server.certificate.write_failed").with("error", e.to_string()),
                ));
            }
        }
    };

    // 证书与私钥成对写入，避免新旧错配；证书走临时文件 + rename 原子写
    //（list_cas 会并发清点该目录，不能让读者看到半截 PEM）
    write_key_file(&target_dir.join(CA_KEY_FILE), &key_pem).await?;
    write_ca_cert_atomic(&target_dir.join(CA_CERT_FILE), &cert_pem).await?;

    Ok(target_dir)
}

/// 仅导入 CA 证书（无私钥）：供"随服务器证书一并导入 CA"场景。
///
/// 先删除既有 ca.key（NotFound 容忍）再写入 ca.pem：若先写证书后删私钥，
/// 中途失败会留下"新证书 + 旧私钥"的错配状态，后续签发产出废证书；
/// 先删私钥失败至多回到"旧证书且不可签发"的一致状态。
pub async fn set_ca_cert_only(cert_pem: Vec<u8>) -> Result<(), CertManagerError> {
    if cert_pem.is_empty() {
        return Err(CertManagerError::Validation(msg(
            "server.certificate.cert_file_missing",
        )));
    }
    let validated = {
        let cert_for_check = cert_pem.clone();
        tokio::task::spawn_blocking(move || validate_ca(&cert_for_check, None))
            .await
            .map_err(|e| {
                CertManagerError::Internal(
                    msg("server.certificate.generate_failed").with("error", e.to_string()),
                )
            })?
    };
    validated?;

    tokio::fs::create_dir_all(CA_DIR).await.map_err(|e| {
        CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e.to_string()),
        )
    })?;
    // 先清除可能存在的旧私钥（删除失败不能静默：残留私钥会破坏
    // "cert-only 不可签发"的不变量），再原子写入新证书（临时文件 + rename，
    // 避免 HTTPS/清点路径并发读到截断的半截 PEM）
    if let Err(e) = tokio::fs::remove_file(ca_key_path()).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(CertManagerError::Internal(
            msg("server.certificate.write_failed").with("error", e),
        ));
    }
    write_ca_cert_atomic(&ca_cert_path(), &cert_pem).await?;
    Ok(())
}

/// 查询 CA 状态（不存在时 available=false）
pub async fn ca_status() -> CaStatus {
    let mut status = CaStatus {
        available: false,
        has_key: false,
        subject_cn: None,
        issuer_cn: None,
        not_before: None,
        not_after: None,
        days_remaining: None,
    };
    let Some(cert_pem) = read_ca_cert().await else {
        return status;
    };
    let Some(meta) = crate::listing::parse_cert_metadata(&cert_pem) else {
        return status;
    };
    status.available = true;
    status.has_key = tokio::fs::try_exists(ca_key_path()).await.unwrap_or(false);
    status.days_remaining = meta
        .not_after
        .map(|na| (na - chrono::Utc::now()).num_days());
    status.subject_cn = meta.subject_cn;
    status.issuer_cn = meta.issuer_cn;
    status.not_before = meta.not_before;
    status.not_after = meta.not_after;
    status
}

/// 读取 CA 证书 PEM（导出用，仅站点根 CA）；不存在返回 NotFound
pub async fn read_ca_cert_pem() -> Result<Vec<u8>, CertManagerError> {
    read_ca_cert()
        .await
        .ok_or_else(|| CertManagerError::NotFound(msg("server.certificate.ca_not_found")))
}

/// CA 列表：站点根 CA（自生成）在前，导入 CA 池按目录名在后。
/// 根 CA 不存在时列表可能只含导入 CA（或为空）。
pub async fn list_cas() -> Vec<CaInfo> {
    let mut list = Vec::new();

    let root = ca_status().await;
    if root.available {
        list.push(CaInfo {
            id: ROOT_CA_ID.to_string(),
            name: root.subject_cn.clone(),
            source: "root".to_string(),
            has_key: root.has_key,
            not_before: root.not_before,
            not_after: root.not_after,
            days_remaining: root.days_remaining,
        });
    }

    let Ok(mut entries) = tokio::fs::read_dir(IMPORT_CA_DIR).await else {
        return list;
    };
    let mut imported = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let id = entry.file_name().to_string_lossy().to_string();
        if !valid_ca_id(&id) || id == ROOT_CA_ID {
            continue;
        }
        let cert_path = import_ca_dir(&id).join(CA_CERT_FILE);
        let Some(cert_pem) = tokio::fs::read(&cert_path).await.ok() else {
            continue;
        };
        let Some(meta) = crate::listing::parse_cert_metadata(&cert_pem) else {
            continue;
        };
        let has_key = tokio::fs::try_exists(import_ca_dir(&id).join(CA_KEY_FILE))
            .await
            .unwrap_or(false);
        imported.push(CaInfo {
            id,
            name: meta.subject_cn,
            source: "imported".to_string(),
            has_key,
            not_before: meta.not_before,
            not_after: meta.not_after,
            days_remaining: meta
                .not_after
                .map(|na| (na - chrono::Utc::now()).num_days()),
        });
    }
    imported.sort_by(|a, b| a.id.cmp(&b.id));
    list.extend(imported);
    list
}

/// 按 CA 标识读取证书与私钥 PEM（签发叶子证书用）。
/// id 为空或 "root" 取站点根 CA；否则取导入 CA 池对应目录。
/// 任一文件缺失返回 None（由调用方决定报错语义）。
pub(crate) async fn load_ca_material_by_id(id: Option<&str>) -> Option<(String, String)> {
    let id = id.unwrap_or(ROOT_CA_ID).trim();
    let (cert_path, key_path) = if id.is_empty() || id == ROOT_CA_ID {
        (ca_cert_path(), ca_key_path())
    } else if valid_ca_id(id) {
        (
            import_ca_dir(id).join(CA_CERT_FILE),
            import_ca_dir(id).join(CA_KEY_FILE),
        )
    } else {
        return None;
    };
    let cert = tokio::fs::read_to_string(cert_path).await.ok()?;
    let key = tokio::fs::read_to_string(key_path).await.ok()?;
    Some((cert, key))
}

/// 按 CA 标识检查证书文件是否存在（生成证书时区分"CA 不存在"与"无私钥"）
pub(crate) async fn ca_cert_exists(id: Option<&str>) -> bool {
    let id = id.unwrap_or(ROOT_CA_ID).trim();
    let path = if id.is_empty() || id == ROOT_CA_ID {
        ca_cert_path()
    } else if valid_ca_id(id) {
        import_ca_dir(id).join(CA_CERT_FILE)
    } else {
        return false;
    };
    tokio::fs::try_exists(path).await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造最小 CA 生成请求
    fn ca_request(cn: &str) -> GenerateCaRequest {
        GenerateCaRequest {
            common_name: cn.to_string(),
            organization: None,
            organizational_unit: None,
            country: None,
            state: None,
            locality: None,
            validity_days: Some(30),
        }
    }

    #[test]
    fn ca请求_serde缺省字段为none() {
        let req: GenerateCaRequest = serde_json::from_str(r#"{"common_name": "FOIMS Root CA"}"#)
            .unwrap_or_else(|e| panic!("反序列化失败: {e}"));
        assert_eq!(req.common_name, "FOIMS Root CA");
        assert!(req.validity_days.is_none());
    }

    /// CA 属性校验：叶子证书（无 BasicConstraints CA:TRUE）应被拒绝
    #[test]
    fn ca校验_非ca证书被拒绝() {
        let key_pair = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "leaf.example.com");
        params.distinguished_name = dn;
        let leaf = params
            .self_signed(&key_pair)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));

        assert!(
            validate_ca(leaf.pem().as_bytes(), None).is_err(),
            "叶子证书不应通过 CA 校验"
        );
    }

    /// CA 属性校验：显式 CA:TRUE 证书应通过
    #[test]
    fn ca校验_ca证书通过() {
        let key_pair = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Test Root CA");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = params
            .self_signed(&key_pair)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));

        assert!(validate_ca(ca.pem().as_bytes(), None).is_ok());
    }

    /// CA 有效期校验：已过期的 CA 应被拒绝
    #[test]
    fn ca校验_过期ca被拒绝() {
        let key_pair = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Expired Root CA");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::days(10);
        params.not_after = now - time::Duration::days(1);
        let ca = params
            .self_signed(&key_pair)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));

        let err = validate_ca(ca.pem().as_bytes(), None)
            .err()
            .unwrap_or_else(|| panic!("过期 CA 应被拒绝"));
        match err {
            CertManagerError::Validation(m) => assert_eq!(m.key(), "server.certificate.ca_invalid"),
            other => panic!("应为校验错误，实际 {other}"),
        }
    }

    /// CA 请求校验：CN 长度、组织长度、国家代码 trim 后判长度
    #[test]
    fn ca请求校验_长度与国家代码() {
        let mut req = ca_request("FOIMS Root CA");
        assert!(validate_generate_ca_request(&req).is_ok());

        req.common_name = "C".repeat(65);
        assert!(
            validate_generate_ca_request(&req).is_err(),
            "超长 CN 应被拒绝"
        );
        req.common_name = "C".repeat(64);
        assert!(validate_generate_ca_request(&req).is_ok());

        req.organization = Some("O".repeat(65));
        assert!(
            validate_generate_ca_request(&req).is_err(),
            "超长组织应被拒绝"
        );
        req.organization = Some("O".repeat(64));
        assert!(validate_generate_ca_request(&req).is_ok());

        // OU / state / locality 同样限 64 字符
        req.organizational_unit = Some("OU".repeat(33));
        assert!(
            validate_generate_ca_request(&req).is_err(),
            "超长 OU 应被拒绝"
        );
        req.organizational_unit = Some("OU".repeat(32));
        assert!(
            validate_generate_ca_request(&req).is_ok(),
            "64 字符 OU 应合法"
        );

        req.state = Some("S".repeat(65));
        assert!(
            validate_generate_ca_request(&req).is_err(),
            "超长州/省应被拒绝"
        );
        req.state = Some("S".repeat(64));
        assert!(validate_generate_ca_request(&req).is_ok());

        req.locality = Some("L".repeat(65));
        assert!(
            validate_generate_ca_request(&req).is_err(),
            "超长城市应被拒绝"
        );
        req.locality = Some("L".repeat(64));
        assert!(validate_generate_ca_request(&req).is_ok());

        // country 先 trim 再判长度（与 generate.rs 统一口径）
        req.country = Some(" C ".to_string());
        assert!(
            validate_generate_ca_request(&req).is_err(),
            "trim 后 1 位应被拒绝"
        );
        req.country = Some("CN ".to_string());
        assert!(
            validate_generate_ca_request(&req).is_ok(),
            "trim 后 2 位应合法"
        );
    }

    /// 私钥与证书公钥匹配校验：另一把钥匙应被拒绝
    #[test]
    fn 私钥匹配_错配私钥被拒绝() {
        let ca_key = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        let mut params = rcgen::CertificateParams::default();
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, "Match Root CA");
        params.distinguished_name = dn;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let ca = params
            .self_signed(&ca_key)
            .unwrap_or_else(|e| panic!("生成证书失败: {e}"));
        let ca_pem = ca.pem();

        assert!(validate_ca(ca_pem.as_bytes(), Some(&ca_key.serialize_pem())).is_ok());

        let other_key = rcgen::KeyPair::generate().unwrap_or_else(|e| panic!("生成密钥失败: {e}"));
        assert!(validate_ca(ca_pem.as_bytes(), Some(&other_key.serialize_pem())).is_err());
    }

    /// DN 组装：空可选字段被跳过
    #[test]
    fn dn组装_可选字段过滤() {
        let req = ca_request("DN Root CA");
        let _ = build_dn(&req); // 不应 panic
    }
}
