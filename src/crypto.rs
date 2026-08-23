//! AES-GCM 加解密与密钥管理。

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use rand::RngExt;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::OnceLock;

use crate::error::{AppError, msg};
use ipma_common::{log_error, log_info, log_warn};

const NONCE_SIZE: usize = 12;

static ENCRYPTION_KEY: OnceLock<Vec<u8>> = OnceLock::new();

/// 将文件权限设置为 0600,仅所有者可读写,防止敏感密钥被其他用户读取
fn secure_file_permissions(path: &str) {
    if let Err(e) = fs::set_permissions(path, fs::Permissions::from_mode(0o600)) {
        log_warn!("log.crypto.set_permissions_failed", path = path, error = e);
    }
}

fn get_key_paths() -> (String, String) {
    let app_name = "ipma";
    let key_path = format!("/etc/{app_name}/encryption.key");
    let key_backup_path = format!("/etc/{app_name}/encryption.key.backup");
    (key_path, key_backup_path)
}

fn load_encryption_key() -> Vec<u8> {
    let (key_path, key_backup_path) = get_key_paths();

    let Some(key_dir) = Path::new(&key_path).parent() else {
        panic!("无法获取加密密钥目录的父目录: {}", key_path);
    };
    if !key_dir.exists()
        && let Err(e) = fs::create_dir_all(key_dir)
    {
        panic!("创建加密密钥目录失败: {}", e);
    }

    if Path::new(&key_path).exists() {
        match fs::read(&key_path) {
            Ok(key) if key.len() == 32 => {
                if let Err(e) = fs::copy(&key_path, &key_backup_path) {
                    log_warn!("log.crypto.backup_create_failed", error = e);
                } else {
                    secure_file_permissions(&key_backup_path);
                    log_info!("log.crypto.key_backed_up", path = key_backup_path);
                }
                return key;
            }
            Ok(key) => {
                panic!(
                    "加密密钥文件长度不正确（期望32字节，实际{}字节），请重新生成密钥",
                    key.len()
                );
            }
            Err(e) => {
                log_error!("log.crypto.key_read_failed", error = e);
                if Path::new(&key_backup_path).exists() {
                    log_warn!("log.crypto.restore_from_backup");
                    match fs::read(&key_backup_path) {
                        Ok(key) if key.len() == 32 => {
                            log_info!("log.crypto.key_restored");
                            if let Err(e) = fs::write(&key_path, &key) {
                                log_error!("log.crypto.key_resave_failed", error = e);
                            } else {
                                secure_file_permissions(&key_path);
                            }
                            return key;
                        }
                        Ok(key) => {
                            panic!("备份密钥长度也不正确（期望32字节，实际{}字节）", key.len());
                        }
                        Err(e) => {
                            panic!("读取备份密钥也失败: {}", e);
                        }
                    }
                }
                panic!("读取加密密钥文件失败，请检查文件权限");
            }
        }
    }

    if Path::new(&key_backup_path).exists() {
        log_warn!("log.crypto.backup_only_detected");
        match fs::read(&key_backup_path) {
            Ok(key) if key.len() == 32 => {
                log_info!("log.crypto.key_restored");
                if let Err(e) = fs::write(&key_path, &key) {
                    log_error!("log.crypto.key_resave_failed", error = e);
                } else {
                    secure_file_permissions(&key_path);
                }
                return key;
            }
            Ok(_) => {
                log_warn!("log.crypto.backup_length_invalid");
            }
            Err(e) => {
                log_warn!("log.crypto.backup_read_failed", error = e);
            }
        }
    }

    log_info!("log.crypto.key_generating", path = key_path);
    let mut key = vec![0u8; 32];
    rand::rng().fill(&mut key);

    if let Err(e) = fs::write(&key_path, &key) {
        panic!("保存加密密钥失败: {}", e);
    }
    secure_file_permissions(&key_path);

    if let Err(e) = fs::write(&key_backup_path, &key) {
        log_warn!("log.crypto.backup_save_failed", error = e);
    } else {
        secure_file_permissions(&key_backup_path);
    }

    log_info!("log.crypto.key_generated", path = key_path);
    key
}

pub fn get_encryption_key() -> Vec<u8> {
    ENCRYPTION_KEY.get_or_init(load_encryption_key).clone()
}

pub fn check_key_integrity() -> Result<(), String> {
    let key = get_encryption_key();
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

pub fn encrypt_password(password: &str) -> Result<String, AppError> {
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| {
        log_error!("log.crypto.cipher_init_failed", error = e);
        AppError::Internal(msg("server.common.cipher_init_failed").with("error", e))
    })?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = aes_gcm::Nonce::from(nonce_bytes);

    let ciphertext = cipher.encrypt(&nonce, password.as_bytes()).map_err(|e| {
        log_error!("log.crypto.encrypt_failed", error = e);
        AppError::Internal(msg("server.common.encrypt_failed").with("error", e))
    })?;

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);

    Ok(BASE64.encode(&result))
}

pub fn decrypt_password(encrypted_password: &str) -> Result<String, String> {
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| format!("解密失败: 加密密钥长度不正确: {e}"))?;

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
        // 静态排查提示 + 关键长度参数，便于定位密钥不一致类问题
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

pub async fn encrypt_password_async(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || encrypt_password(&password))
        .await
        .map_err(|e| {
            AppError::Internal(msg("server.common.encrypt_task_failed").with("error", e))
        })?
}

pub async fn decrypt_password_async(encrypted: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || decrypt_password(&encrypted))
        .await
        .map_err(|e| format!("解密任务失败: {e}"))?
}

pub async fn decrypt_credential_async(value: Option<String>) -> Result<Option<String>, AppError> {
    match value {
        Some(v) => decrypt_password_async(v)
            .await
            .map(Some)
            .map_err(|e| AppError::Internal(msg("server.common.decrypt_failed").with("error", e))),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let password = "test_password_123";
        let encrypted = encrypt_password(password).unwrap();
        let decrypted = decrypt_password(&encrypted).unwrap();
        assert_eq!(password, decrypted);
    }

    #[test]
    fn test_encrypt_produces_different_output() {
        let password = "test_password_123";
        let encrypted1 = encrypt_password(password).unwrap();
        let encrypted2 = encrypt_password(password).unwrap();
        assert_ne!(encrypted1, encrypted2);

        assert_eq!(password, decrypt_password(&encrypted1).unwrap());
        assert_eq!(password, decrypt_password(&encrypted2).unwrap());
    }

    #[test]
    fn test_encrypt_decrypt_empty_string() {
        // 空明文也应可加解密往返（密文仅含 nonce + 16 字节认证标签）
        let encrypted = encrypt_password("").unwrap_or_else(|e| panic!("空串加密失败: {e}"));
        let decrypted =
            decrypt_password(&encrypted).unwrap_or_else(|e| panic!("空串解密失败: {e}"));
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_encrypt_decrypt_multibyte_utf8() {
        // 多字节 UTF-8（中文 + emoji）往返保持字节一致
        let password = "超级管理员密码🔐P@ssw0rd！";
        let encrypted = encrypt_password(password).unwrap_or_else(|e| panic!("加密失败: {e}"));
        let decrypted = decrypt_password(&encrypted).unwrap_or_else(|e| panic!("解密失败: {e}"));
        assert_eq!(password, decrypted);
    }

    #[test]
    fn test_encrypt_ciphertext_layout() {
        // 密文结构：Base64(12 字节 nonce || 明文 || 16 字节 GCM 标签)
        let plaintext = "0123456789abcdef"; // 16 字节
        let encrypted = encrypt_password(plaintext).unwrap_or_else(|e| panic!("加密失败: {e}"));
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
        let result = decrypt_password("!!!not-base64!!!");
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
        let result = decrypt_password(&short);
        let err = result.err().unwrap_or_default();
        assert!(err.contains("长度不足"), "错误信息应指明长度不足: {err}");
        // 恰好 12 字节（只有 nonce、无密文）交给 AES-GCM 解密失败分支
        let nonce_only = BASE64.encode([0u8; NONCE_SIZE]);
        assert!(
            decrypt_password(&nonce_only).is_err(),
            "仅有 nonce 应解密失败"
        );
    }

    #[test]
    fn test_decrypt_rejects_tampered_ciphertext() {
        // 篡改密文区任一字节，GCM 认证标签校验必须失败
        let encrypted = encrypt_password("tamper-me").unwrap_or_else(|e| panic!("加密失败: {e}"));
        let mut decoded = BASE64
            .decode(&encrypted)
            .unwrap_or_else(|e| panic!("Base64 解码失败: {e}"));
        let last = decoded.len() - 1;
        decoded[last] ^= 0xFF;
        let tampered = BASE64.encode(&decoded);
        let result = decrypt_password(&tampered);
        let err = result.err().unwrap_or_default();
        assert!(
            err.contains("AES-GCM"),
            "错误信息应为 AES-GCM 解密错误: {err}"
        );
    }

    #[test]
    fn test_decrypt_rejects_tampered_nonce() {
        // 篡改 nonce 同样导致解密失败
        let encrypted =
            encrypt_password("tamper-nonce").unwrap_or_else(|e| panic!("加密失败: {e}"));
        let mut decoded = BASE64
            .decode(&encrypted)
            .unwrap_or_else(|e| panic!("Base64 解码失败: {e}"));
        decoded[0] ^= 0x01;
        let tampered = BASE64.encode(&decoded);
        assert!(
            decrypt_password(&tampered).is_err(),
            "篡改 nonce 后应解密失败"
        );
    }

    #[test]
    fn test_encrypt_long_text_roundtrip() {
        // 长文本（约 100KB）分段无关地往返成功（AES-GCM 单次加密无分块限制）
        let long_text = "长".repeat(32_768); // 98_304 字节
        let encrypted = encrypt_password(&long_text).unwrap_or_else(|e| panic!("加密失败: {e}"));
        let decrypted = decrypt_password(&encrypted).unwrap_or_else(|e| panic!("解密失败: {e}"));
        assert_eq!(decrypted, long_text);
    }

    #[test]
    fn test_different_plaintexts_produce_different_ciphertexts() {
        let enc1 = encrypt_password("password-a").unwrap_or_else(|e| panic!("加密失败: {e}"));
        let enc2 = encrypt_password("password-b").unwrap_or_else(|e| panic!("加密失败: {e}"));
        assert_ne!(enc1, enc2, "不同明文不应产生相同密文");
    }

    #[test]
    fn test_get_encryption_key_shape_and_stability() {
        // 密钥长度必须为 32 字节（AES-256），且 OnceLock 保证多次获取一致
        let key1 = get_encryption_key();
        let key2 = get_encryption_key();
        assert_eq!(key1.len(), 32, "加密密钥应为 32 字节");
        assert_eq!(key1, key2, "同一进程内密钥应保持稳定");
    }

    #[test]
    fn test_check_key_integrity_returns_ok() {
        // 密钥可用时完整性检查应通过（长度校验 + 备份比对仅记日志）
        let result = check_key_integrity();
        assert!(result.is_ok(), "密钥完整性检查应通过");
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_async_roundtrip() {
        // 异步包装与同步实现行为一致
        let plaintext = "async-credential-测试";
        let encrypted = encrypt_password_async(plaintext.to_string())
            .await
            .unwrap_or_else(|e| panic!("异步加密失败: {e}"));
        let decrypted = decrypt_password_async(encrypted)
            .await
            .unwrap_or_else(|e| panic!("异步解密失败: {e}"));
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_decrypt_credential_async_none_passthrough() {
        // None 输入直接透传，不触发加解密
        let result = decrypt_credential_async(None).await;
        assert!(result.is_ok(), "None 输入应成功透传");
        assert_eq!(result.ok().flatten(), None);
    }

    #[tokio::test]
    async fn test_decrypt_credential_async_invalid_value() {
        // 非法密文经异步链路返回错误
        let result = decrypt_credential_async(Some("!!!bad-base64!!!".to_string())).await;
        assert!(result.is_err(), "非法密文应返回错误");
    }
}
