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
}
