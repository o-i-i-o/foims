//! AES-GCM 加解密与密钥管理。

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use rand::RngExt;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use crate::{AppError, msg};
use crate::{log_error, log_info, log_warn};

/// GCM 标准 nonce 长度（96 bit）；密文布局为 Base64(nonce || 密文+标签)，
/// 属存量数据格式约束，改动会导致历史密文不可解。
const NONCE_SIZE: usize = 12;

/// 恒定时间字符串比较（防时序侧信道）。
///
/// 长度不同直接返回 false：长度信息本身不属于敏感范围
/// （foims-auth 与 foims-init 共用的唯一定义）。
#[must_use]
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

/// bcrypt 密码哈希（spawn_blocking 包裹，避免阻塞 tokio 工作线程）。
///
/// foims-auth 与 foims-init 共用的唯一定义；72 字节入口校验由
/// `validation::validate_password_max_bytes` 在请求层完成。
pub async fn hash_password(password: &str) -> Result<String, AppError> {
    let password = password.to_string();
    tokio::task::spawn_blocking(move || bcrypt::hash(&password, bcrypt::DEFAULT_COST))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.common.password_hash_task_failed").with("error", e))
        })?
        .map_err(|err| {
            AppError::Internal(msg("server.common.password_hash_failed").with("error", err))
        })
}

/// 加密密钥加载/保存失败原因。
///
/// 动态参数统一为 String（io::Error 不可克隆）：错误值需存入
/// `OnceLock<Result<_, _>>` 供后续调用方映射，因此派生 Clone。
#[derive(Debug, Clone, thiserror::Error)]
pub enum CryptoKeyError {
    #[error("无法获取加密密钥目录的父目录: {0}")]
    KeyDirInvalid(String),
    #[error("创建加密密钥目录失败: {0}")]
    KeyDirCreateFailed(String),
    #[error("加密密钥文件长度不正确（期望32字节，实际{0}字节），请重新生成密钥")]
    KeyLengthInvalid(usize),
    #[error("读取加密密钥文件失败: {0}")]
    KeyReadFailed(String),
    #[error(
        "备份密钥长度也不正确（期望32字节，实际{0}字节）。为避免生成新密钥覆盖唯一备份、导致存量密文永久不可解密，拒绝启动；请手工恢复 /etc/foims/encryption.key 后重试"
    )]
    BackupKeyLengthInvalid(usize),
    #[error(
        "读取备份密钥也失败: {0}。为避免生成新密钥覆盖唯一备份、导致存量密文永久不可解密，拒绝启动；请手工恢复 /etc/foims/encryption.key 后重试"
    )]
    BackupKeyReadFailed(String),
    #[error("保存加密密钥失败: {0}")]
    KeyWriteFailed(String),
}

/// 进程级加密密钥：`Result` 入锁，加载失败在使用处映射为错误而非 panic。
static ENCRYPTION_KEY: OnceLock<Result<Vec<u8>, CryptoKeyError>> = OnceLock::new();

/// 以 0600 权限写入密钥文件（覆盖已存在内容）。
/// 直接用带 mode 的 OpenOptions 创建，消除「先写后 chmod」窗口期内
/// 其他本地用户读到密钥的可能（security-review I-6）。
/// 返回写入是否成功。
fn write_key_file(path: &str, key: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .mode(0o600)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(key)
}

/// 尽力写入密钥文件：失败仅告警（用于备份等非关键副本）
fn write_key_file_atomic(path: &str, key: &[u8]) {
    if let Err(e) = write_key_file(path, key) {
        log_warn!("log.crypto.backup_create_failed", error = e);
    } else {
        log_info!("log.crypto.key_backed_up", path = path);
    }
}

/// 由主密钥文件路径推导备份文件路径（同名 + `.backup` 后缀）。
fn backup_path_for(key_path: &str) -> String {
    format!("{key_path}.backup")
}

/// 生产密钥文件路径：`(主密钥, 备份密钥)`，主密钥固定 /etc/foims/encryption.key，
/// 备份名按 [`backup_path_for`] 规则派生。
fn get_key_paths() -> (String, String) {
    let app_name = "foims";
    let key_path = format!("/etc/{app_name}/encryption.key");
    let key_backup_path = backup_path_for(&key_path);
    (key_path, key_backup_path)
}

/// 生成 32 字节（AES-256）随机密钥。
fn generate_key_material() -> Vec<u8> {
    let mut key = vec![0u8; 32];
    rand::rng().fill(&mut key);
    key
}

/// 密钥材料加载核心：全部路径参数化，不落进程级缓存，供生产装配与
/// 单元测试（临时目录）共用同一实现，避免测试依赖真实 /etc 密钥文件。
///
/// 返回值第二项标记「本次是否新生成密钥」；分支语义与
/// [`load_encryption_key`] 完全一致（fail-closed 规则见该函数注释）。
fn load_key_material(
    key_path: &str,
    key_backup_path: &str,
) -> Result<(Vec<u8>, bool), CryptoKeyError> {
    if Path::new(key_path).exists() {
        match fs::read(key_path) {
            Ok(key) if key.len() == 32 => {
                write_key_file_atomic(key_backup_path, &key);
                return Ok((key, false));
            }
            Ok(key) => {
                return Err(CryptoKeyError::KeyLengthInvalid(key.len()));
            }
            Err(e) => {
                log_error!("log.crypto.key_read_failed", error = e);
                if Path::new(key_backup_path).exists() {
                    log_warn!("log.crypto.restore_from_backup");
                    match fs::read(key_backup_path) {
                        Ok(key) if key.len() == 32 => {
                            log_info!("log.crypto.key_restored");
                            write_key_file_atomic(key_path, &key);
                            return Ok((key, false));
                        }
                        Ok(key) => {
                            return Err(CryptoKeyError::BackupKeyLengthInvalid(key.len()));
                        }
                        Err(e) => {
                            return Err(CryptoKeyError::BackupKeyReadFailed(e.to_string()));
                        }
                    }
                }
                return Err(CryptoKeyError::KeyReadFailed(e.to_string()));
            }
        }
    }

    if Path::new(key_backup_path).exists() {
        log_warn!("log.crypto.backup_only_detected");
        match fs::read(key_backup_path) {
            Ok(key) if key.len() == 32 => {
                log_info!("log.crypto.key_restored");
                write_key_file_atomic(key_path, &key);
                return Ok((key, false));
            }
            // fail-closed：主密钥缺失且备份不可用（读取失败/长度非法）时
            // 必须拒绝启动，绝不生成新密钥覆盖备份——旧密钥唯一残存副本被
            // 销毁后，SMTP/SNMP 等存量 AES-GCM 密文将永久不可解密
            Ok(key) => {
                return Err(CryptoKeyError::BackupKeyLengthInvalid(key.len()));
            }
            Err(e) => {
                return Err(CryptoKeyError::BackupKeyReadFailed(e.to_string()));
            }
        }
    }

    log_info!("log.crypto.key_generating", path = key_path);
    let key = generate_key_material();

    // 新密钥必须成功落盘：否则下次启动会再生成新密钥，历史密文全部无法解密
    write_key_file(key_path, &key).map_err(|e| CryptoKeyError::KeyWriteFailed(e.to_string()))?;
    write_key_file_atomic(key_backup_path, &key);

    log_info!("log.crypto.key_generated", path = key_path);
    Ok((key, true))
}

/// 加载（必要时生成）加密密钥。
///
/// 生产路径禁止 panic：所有失败分支均以 `CryptoKeyError` 返回，
/// 由调用方决定退出或报错。
fn load_encryption_key() -> Result<Vec<u8>, CryptoKeyError> {
    let (key_path, key_backup_path) = get_key_paths();

    let Some(key_dir) = Path::new(&key_path).parent() else {
        return Err(CryptoKeyError::KeyDirInvalid(key_path.clone()));
    };
    if !key_dir.exists()
        && let Err(e) = fs::create_dir_all(key_dir)
    {
        return Err(CryptoKeyError::KeyDirCreateFailed(e.to_string()));
    }

    load_key_material(&key_path, &key_backup_path).map(|(key, _generated)| key)
}

/// 获取进程级加密密钥（首次调用时加载，之后复用缓存结果）。
///
/// 加载失败时返回错误：调用方（加解密/完整性检查）据此向上传递错误，
/// 主程序启动路径据此以 Err 退出，不再 panic。
pub fn ensure_encryption_key() -> Result<&'static [u8], CryptoKeyError> {
    let initialized = ENCRYPTION_KEY.get_or_init(load_encryption_key);
    initialized.as_deref().map_err(std::clone::Clone::clone)
}

/// 密钥完整性自检（供启动期调用）：主密钥长度非法返回 Err；
/// 备份缺失/不可读/与主密钥不一致仅记告警、不阻断启动。
pub fn check_key_integrity() -> Result<(), String> {
    let key = ensure_encryption_key().map_err(|e| e.to_string())?;
    if key.len() != 32 {
        return Err(format!("密钥长度不正确: {}字节（期望32字节）", key.len()));
    }

    let (key_path, backup_path) = get_key_paths();
    log_info!("log.crypto.integrity_check_passed");
    log_info!("log.crypto.main_key_path", path = key_path);
    log_info!("log.crypto.backup_key_path", path = backup_path);

    if Path::new(&backup_path).exists() {
        match fs::read(&backup_path) {
            Ok(backup_key) if backup_key == key => {
                log_info!("log.crypto.backup_integrity_passed");
            }
            Ok(_) => {
                log_warn!("log.crypto.backup_mismatch");
                log_warn!("log.crypto.backup_mismatch_recent");
                log_warn!("log.crypto.backup_mismatch_corruption");
            }
            Err(e) => {
                log_warn!("log.crypto.backup_read_failed", error = e);
            }
        }
    } else {
        log_warn!("log.crypto.backup_missing");
    }

    Ok(())
}

/// AES-GCM 加密核心：密钥与 nonce 显式注入，不落进程级密钥缓存，
/// 供 [`encrypt_password`] 装配与单元测试（固定测试密钥）共用同一实现。
fn encrypt_with_key(
    key: &[u8],
    nonce_bytes: [u8; NONCE_SIZE],
    plaintext: &str,
) -> Result<String, AppError> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| {
        log_error!("log.crypto.cipher_init_failed", error = e);
        AppError::Internal(msg("server.common.cipher_init_failed").with("error", e))
    })?;

    let nonce = aes_gcm::Nonce::from(nonce_bytes);
    let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes()).map_err(|e| {
        log_error!("log.crypto.encrypt_failed", error = e);
        AppError::Internal(msg("server.common.encrypt_failed").with("error", e))
    })?;

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);

    Ok(BASE64.encode(&result))
}

/// AES-GCM 解密核心：密钥显式注入（输入校验与错误语义与
/// [`decrypt_password`] 一致），供其装配与单元测试共用同一实现。
fn decrypt_with_key(key: &[u8], encrypted_password: &str) -> Result<String, String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| format!("解密失败: 加密密钥长度不正确: {e}"))?;

    let decoded = BASE64
        .decode(encrypted_password)
        .map_err(|e| format!("解密失败: Base64解码错误: {e}"))?;

    if decoded.len() < NONCE_SIZE {
        return Err(format!(
            "解密失败: 密文长度不足 (期望>{}字节, 实际{}字节)",
            NONCE_SIZE,
            decoded.len()
        ));
    }

    let (nonce_bytes, ciphertext) = decoded.split_at(NONCE_SIZE);
    let nonce_arr: [u8; NONCE_SIZE] = nonce_bytes
        .try_into()
        .map_err(|_| format!("解密失败: Nonce长度不正确 (期望{}字节)", NONCE_SIZE))?;
    let nonce = aes_gcm::Nonce::from(nonce_arr);

    let plaintext = cipher.decrypt(&nonce, ciphertext).map_err(|_| {
        log_warn!("log.crypto.decrypt_failed_hint");
        log_warn!(
            "log.crypto.decrypt_detail",
            key_len = key.len(),
            nonce_len = nonce_bytes.len(),
            ciphertext_len = ciphertext.len()
        );
        "解密失败: AES-GCM解密错误".to_string()
    })?;

    String::from_utf8(plaintext).map_err(|e| format!("解密失败: UTF-8解码错误: {e}"))
}

/// AES-GCM 加密（进程密钥 + 每次随机 nonce）：输出 Base64(nonce || 密文+标签)。
/// 密钥加载失败（fail-closed 规则见 load_key_material 注释）映射为 Internal 错误。
pub fn encrypt_password(password: &str) -> Result<String, AppError> {
    let key = ensure_encryption_key().map_err(|e| {
        log_error!("log.crypto.key_load_failed", error = e);
        AppError::Internal(msg("server.common.cipher_init_failed").with("error", e))
    })?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill(&mut nonce_bytes);
    encrypt_with_key(key, nonce_bytes, password)
}

/// [`encrypt_password`] 输出的解密；密钥、Base64、长度、GCM 标签、UTF-8 任一
/// 校验失败均以错误文本返回。
pub fn decrypt_password(encrypted_password: &str) -> Result<String, String> {
    let key = ensure_encryption_key().map_err(|e| format!("解密失败: 加密密钥加载失败: {e}"))?;
    decrypt_with_key(key, encrypted_password)
}

/// [`encrypt_password`] 的 spawn_blocking 版本（避免阻塞 tokio 工作线程）。
pub async fn encrypt_password_async(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || encrypt_password(&password))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.common.encrypt_task_failed").with("error", e))
        })?
}

/// [`decrypt_password`] 的 spawn_blocking 版本（避免阻塞 tokio 工作线程）。
pub async fn decrypt_password_async(encrypted: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || decrypt_password(&encrypted))
        .await
        .map_err(|e| format!("解密任务失败: {e}"))?
}

/// 按借用接收待解密凭据，由调用方决定所有权，避免调用点为跨 await 传递而克隆
pub async fn decrypt_credential_async(value: Option<&str>) -> Result<Option<String>, AppError> {
    match value {
        Some(v) => decrypt_password_async(v.to_string())
            .await
            .map(Some)
            .map_err(|e| AppError::Internal(msg("server.common.decrypt_failed").with("error", e))),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// 固定测试密钥（32 字节）：仅测试内部使用，非生产凭据
    fn test_key() -> Vec<u8> {
        vec![0x42u8; 32]
    }

    /// 构造一次性临时目录（无 tempfile 依赖，沿用 foims-services 先例；
    /// 纳秒时戳避免同进程多线程测试撞名）
    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "foims-crypto-test-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        // 目录创建失败时后续断言自然红灯，不回退静默通过
        fs::create_dir_all(&base).ok();
        base
    }

    /// 临时目录内的主/备密钥路径（与生产同名规则）
    fn key_paths_in(base: &Path) -> (String, String) {
        let key_path = base.join("encryption.key").to_string_lossy().into_owned();
        let backup_path = backup_path_for(&key_path);
        (key_path, backup_path)
    }

    fn test_nonce(fill: u8) -> [u8; NONCE_SIZE] {
        [fill; NONCE_SIZE]
    }

    /// 用固定密钥与 nonce 加密（密码学核心的直接调用，不落盘不碰 /etc）
    fn encrypt_with(key: &[u8], nonce: [u8; NONCE_SIZE], plaintext: &str) -> String {
        encrypt_with_key(key, nonce, plaintext).unwrap_or_else(|e| panic!("测试加密应成功: {e}"))
    }

    // 加解密往返（密钥注入，无文件系统依赖）

    #[test]
    fn test_encrypt_decrypt() {
        let password = "test_password_123";
        let encrypted = encrypt_with(&test_key(), test_nonce(1), password);
        let decrypted =
            decrypt_with_key(&test_key(), &encrypted).unwrap_or_else(|e| panic!("解密失败: {e}"));
        assert_eq!(password, decrypted);
    }

    #[test]
    fn test_encrypt_produces_different_output() {
        // nonce 不同则密文不同，且均可用同一密钥解回
        let password = "test_password_123";
        let encrypted1 = encrypt_with(&test_key(), test_nonce(7), password);
        let encrypted2 = encrypt_with(&test_key(), test_nonce(8), password);
        assert_ne!(encrypted1, encrypted2);

        assert_eq!(
            password,
            decrypt_with_key(&test_key(), &encrypted1).unwrap_or_default()
        );
        assert_eq!(
            password,
            decrypt_with_key(&test_key(), &encrypted2).unwrap_or_default()
        );
    }

    #[test]
    fn test_encrypt_decrypt_empty_string() {
        // 空明文也应可加解密往返（密文仅含 nonce + 16 字节认证标签）
        let encrypted = encrypt_with(&test_key(), test_nonce(2), "");
        let decrypted = decrypt_with_key(&test_key(), &encrypted)
            .unwrap_or_else(|e| panic!("空串解密失败: {e}"));
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_encrypt_decrypt_multibyte_utf8() {
        let password = "超级管理员密码🔐P@ssw0rd！";
        let encrypted = encrypt_with(&test_key(), test_nonce(3), password);
        let decrypted =
            decrypt_with_key(&test_key(), &encrypted).unwrap_or_else(|e| panic!("解密失败: {e}"));
        assert_eq!(password, decrypted);
    }

    #[test]
    fn test_encrypt_ciphertext_layout() {
        // 密文结构：Base64(12 字节 nonce || 明文 || 16 字节 GCM 标签)
        let plaintext = "0123456789abcdef"; // 16 字节
        let encrypted = encrypt_with(&test_key(), test_nonce(4), plaintext);
        let decoded = BASE64
            .decode(&encrypted)
            .unwrap_or_else(|e| panic!("密文应为合法 Base64: {e}"));
        assert_eq!(
            decoded.len(),
            NONCE_SIZE + 16 + 16,
            "长度应为 nonce+明文+标签"
        );
    }

    #[test]
    fn test_decrypt_rejects_invalid_base64() {
        let result = decrypt_with_key(&test_key(), "!!!not-base64!!!");
        let err = result.err().unwrap_or_default();
        assert!(
            err.contains("Base64"),
            "错误信息应指明 Base64 解码失败: {err}"
        );
    }

    #[test]
    fn test_decrypt_rejects_short_ciphertext() {
        // 解码后不足 nonce 长度（12 字节）应直接失败
        let short = BASE64.encode([0u8; NONCE_SIZE - 1]);
        let result = decrypt_with_key(&test_key(), &short);
        let err = result.err().unwrap_or_default();
        assert!(err.contains("长度不足"), "错误信息应指明长度不足: {err}");
        // 恰好 12 字节（只有 nonce、无密文）交给 AES-GCM 解密失败分支
        let nonce_only = BASE64.encode([0u8; NONCE_SIZE]);
        assert!(
            decrypt_with_key(&test_key(), &nonce_only).is_err(),
            "仅有 nonce 应解密失败"
        );
    }

    #[test]
    fn test_decrypt_rejects_tampered_ciphertext() {
        // 篡改密文区任一字节，GCM 认证标签校验必须失败
        let encrypted = encrypt_with(&test_key(), test_nonce(5), "tamper-me");
        let mut decoded = BASE64
            .decode(&encrypted)
            .unwrap_or_else(|e| panic!("Base64 解码失败: {e}"));
        let last = decoded.len() - 1;
        decoded[last] ^= 0xFF;
        let tampered = BASE64.encode(&decoded);
        let result = decrypt_with_key(&test_key(), &tampered);
        let err = result.err().unwrap_or_default();
        assert!(
            err.contains("AES-GCM"),
            "错误信息应为 AES-GCM 解密错误: {err}"
        );
    }

    #[test]
    fn test_decrypt_rejects_tampered_nonce() {
        let encrypted = encrypt_with(&test_key(), test_nonce(6), "tamper-nonce");
        let mut decoded = BASE64
            .decode(&encrypted)
            .unwrap_or_else(|e| panic!("Base64 解码失败: {e}"));
        decoded[0] ^= 0x01;
        let tampered = BASE64.encode(&decoded);
        assert!(
            decrypt_with_key(&test_key(), &tampered).is_err(),
            "篡改 nonce 后应解密失败"
        );
    }

    #[test]
    fn test_encrypt_long_text_roundtrip() {
        // 长文本（约 100KB）分段无关地往返成功（AES-GCM 单次加密无分块限制）
        let long_text = "长".repeat(32_768); // 98_304 字节
        let encrypted = encrypt_with(&test_key(), test_nonce(9), &long_text);
        let decrypted =
            decrypt_with_key(&test_key(), &encrypted).unwrap_or_else(|e| panic!("解密失败: {e}"));
        assert_eq!(decrypted, long_text);
    }

    #[test]
    fn test_different_plaintexts_produce_different_ciphertexts() {
        let enc1 = encrypt_with(&test_key(), test_nonce(10), "password-a");
        let enc2 = encrypt_with(&test_key(), test_nonce(11), "password-b");
        assert_ne!(enc1, enc2, "不同明文不应产生相同密文");
    }

    #[test]
    fn test_decrypt_rejects_wrong_key() {
        // 密钥不一致时 GCM 标签校验必须失败（防默默用错密钥解密）
        let encrypted = encrypt_with(&test_key(), test_nonce(12), "secret");
        let other_key = vec![0x99u8; 32];
        assert!(
            decrypt_with_key(&other_key, &encrypted).is_err(),
            "错误密钥不应能解密"
        );
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_async_roundtrip() {
        // spawn_blocking 包裹链路与同步核心行为一致（密钥注入版，不碰 /etc）
        let plaintext = "async-credential-测试";
        let key = test_key();
        let encrypted = tokio::task::spawn_blocking({
            let key = key.clone();
            let plaintext = plaintext.to_string();
            move || encrypt_with_key(&key, test_nonce(13), &plaintext)
        })
        .await
        .unwrap_or_else(|e| panic!("异步加密任务失败: {e}"))
        .unwrap_or_else(|e| panic!("异步加密失败: {e}"));
        let decrypted = tokio::task::spawn_blocking(move || decrypt_with_key(&key, &encrypted))
            .await
            .unwrap_or_else(|e| panic!("异步解密任务失败: {e}"))
            .unwrap_or_else(|e| panic!("异步解密失败: {e}"));
        assert_eq!(decrypted, plaintext);
    }

    // 密钥加载核心（临时目录，全分支覆盖）

    #[test]
    fn test_load_key_material_generates_and_persists() {
        // 主/备均缺失：新生成 32 字节密钥，以 0600 落盘并同步写备份
        let base = temp_dir("gen");
        let (key_path, backup_path) = key_paths_in(&base);
        let (key, generated) = load_key_material(&key_path, &backup_path)
            .unwrap_or_else(|e| panic!("首次加载应生成密钥: {e}"));
        assert!(generated, "应标记为新生成");
        assert_eq!(key.len(), 32, "加密密钥应为 32 字节");
        assert_eq!(fs::read(&key_path).unwrap_or_default(), key);
        assert_eq!(fs::read(&backup_path).unwrap_or_default(), key);
        let mode = fs::metadata(&key_path).map(|m| m.permissions().mode()).ok();
        assert_eq!(mode.map(|m| m & 0o777), Some(0o600), "密钥文件应为 0600");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_load_key_material_reuses_existing_key() {
        // 主密钥已存在且长度合法：直接复用，不重新生成
        let base = temp_dir("reuse");
        let (key_path, backup_path) = key_paths_in(&base);
        let existing = vec![0x11u8; 32];
        write_key_file(&key_path, &existing).unwrap_or_else(|e| panic!("预置主密钥失败: {e}"));
        let (key, generated) = load_key_material(&key_path, &backup_path)
            .unwrap_or_else(|e| panic!("已有密钥加载应成功: {e}"));
        assert!(!generated, "复用既有密钥不应标记新生成");
        assert_eq!(key, existing);
        // 加载成功后备份应被同步刷新
        assert_eq!(fs::read(&backup_path).unwrap_or_default(), existing);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_load_key_material_restores_from_backup_only() {
        // 仅备份存在：恢复密钥并回写主文件
        let base = temp_dir("restore");
        let (key_path, backup_path) = key_paths_in(&base);
        let backup_key = vec![0x22u8; 32];
        write_key_file(&backup_path, &backup_key)
            .unwrap_or_else(|e| panic!("预制备份密钥失败: {e}"));
        let (key, generated) = load_key_material(&key_path, &backup_path)
            .unwrap_or_else(|e| panic!("备份恢复应成功: {e}"));
        assert!(!generated, "备份恢复不是新生成");
        assert_eq!(key, backup_key);
        assert_eq!(
            fs::read(&key_path).unwrap_or_default(),
            backup_key,
            "应回写主文件"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_load_key_material_rejects_bad_lengths() {
        // 主密钥长度非法/仅备份且备份长度非法：均拒绝，绝不生成新密钥覆盖备份
        let base = temp_dir("length");
        let (key_path, backup_path) = key_paths_in(&base);

        fs::write(&key_path, b"too-short").unwrap_or_else(|e| panic!("写入短密钥失败: {e}"));
        match load_key_material(&key_path, &backup_path) {
            Err(CryptoKeyError::KeyLengthInvalid(len)) => assert_eq!(len, 9),
            other => panic!("应报主密钥长度非法，实际: {other:?}"),
        }
        let _ = fs::remove_file(&key_path);

        fs::write(&backup_path, vec![0u8; 31]).unwrap_or_else(|e| panic!("写入短备份失败: {e}"));
        match load_key_material(&key_path, &backup_path) {
            Err(CryptoKeyError::BackupKeyLengthInvalid(len)) => assert_eq!(len, 31),
            other => panic!("应报备份密钥长度非法，实际: {other:?}"),
        }
        // fail-closed：备份内容不得被新密钥覆盖
        assert_eq!(
            fs::read(&backup_path).unwrap_or_default(),
            vec![0u8; 31],
            "拒绝启动路径不应碰备份文件"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_load_key_material_fail_closed_on_unreadable_backup() {
        // 仅备份存在但不可读：返回 BackupKeyReadFailed，绝不生成新密钥覆盖
        let base = temp_dir("unreadable");
        let (key_path, backup_path) = key_paths_in(&base);
        // 用同名目录冒充备份文件：非特权下 fs::read 必失败（不依赖 chmod，root 也读不了）
        fs::create_dir_all(&backup_path).unwrap_or_else(|e| panic!("构造不可读备份失败: {e}"));
        match load_key_material(&key_path, &backup_path) {
            Err(CryptoKeyError::BackupKeyReadFailed(_)) => {}
            other => panic!("应报备份读取失败，实际: {other:?}"),
        }
        assert!(!Path::new(&key_path).exists(), "不应生成新主密钥文件");
        let _ = fs::remove_dir_all(&base);
    }

    // 真实 /etc 密钥链路（环境不满足时跳过，不断言）

    /// 真实密钥目录是否可正常加载（CI/非 root 开发机下不可写，此时
    /// 上面的临时目录测试已覆盖全部失败分支，这里仅验证生产装配）
    fn real_key_available() -> bool {
        ensure_encryption_key().is_ok()
    }

    #[test]
    fn test_get_encryption_key_shape_and_stability() {
        // 密钥可用时：长度必须为 32 字节（AES-256），OnceLock 保证多次获取一致
        if !real_key_available() {
            return;
        }
        let key1 = ensure_encryption_key().unwrap_or_default();
        let key2 = ensure_encryption_key().unwrap_or_default();
        assert_eq!(key1.len(), 32, "加密密钥应为 32 字节");
        assert_eq!(key1, key2, "同一进程内密钥应保持稳定");
    }

    #[test]
    fn test_check_key_integrity_returns_ok() {
        // 密钥可用时完整性检查应通过（长度校验 + 备份比对仅记日志）
        if !real_key_available() {
            return;
        }
        let result = check_key_integrity();
        assert!(result.is_ok(), "密钥完整性检查应通过");
    }

    #[tokio::test]
    async fn test_decrypt_credential_async_none_passthrough() {
        // None 输入直接透传，不触发加解密（无密钥依赖，任何环境可跑）
        let result = decrypt_credential_async(None).await;
        assert!(result.is_ok(), "None 输入应成功透传");
        assert_eq!(result.ok().flatten(), None);
    }

    #[tokio::test]
    async fn test_decrypt_credential_async_invalid_value() {
        // 非法密文经异步链路返回错误（公开链路，需真实密钥；环境不满足时跳过）
        if !real_key_available() {
            return;
        }
        let result = decrypt_credential_async(Some("!!!bad-base64!!!")).await;
        assert!(result.is_err(), "非法密文应返回错误");
    }
}
