use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use rand::RngExt;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;
use tracing::{error, info, warn};

const NONCE_SIZE: usize = 12;

static ENCRYPTION_KEY: OnceLock<Vec<u8>> = OnceLock::new();

fn get_key_paths() -> (String, String) {
    let app_name = "ipma";
    let key_path = format!("/etc/{app_name}/encryption.key");
    let key_backup_path = format!("/etc/{app_name}/encryption.key.backup");
    (key_path, key_backup_path)
}

fn load_encryption_key() -> Vec<u8> {
    let (key_path, key_backup_path) = get_key_paths();

    let key_dir = Path::new(&key_path).parent().expect("无法获取加密密钥目录的父目录");
    if !key_dir.exists()
        && let Err(e) = fs::create_dir_all(key_dir) {
            error!("创建加密密钥目录失败: {}", e);
            panic!("创建加密密钥目录失败: {e}");
        }

    if Path::new(&key_path).exists() {
        match fs::read(&key_path) {
            Ok(key) if key.len() == 32 => {
                if let Err(e) = fs::copy(&key_path, &key_backup_path) {
                    warn!("无法创建密钥备份: {}，不影响正常运行", e);
                } else {
                    info!("加密密钥已备份到: {}", key_backup_path);
                }
                return key;
            }
            Ok(key) => {
                error!(
                    "加密密钥文件长度不正确（期望32字节，实际{}字节），请重新生成密钥",
                    key.len()
                );
                panic!(
                    "加密密钥文件长度不正确（期望32字节，实际{}字节）",
                    key.len()
                );
            }
            Err(e) => {
                error!("读取加密密钥文件失败: {}", e);
                if Path::new(&key_backup_path).exists() {
                    warn!("尝试从备份密钥恢复...");
                    match fs::read(&key_backup_path) {
                        Ok(key) if key.len() == 32 => {
                            info!("成功从备份恢复密钥!");
                            if let Err(e) = fs::write(&key_path, &key) {
                                error!("恢复密钥后无法重新保存: {}", e);
                            }
                            return key;
                        }
                        Ok(key) => {
                            error!("备份密钥长度也不正确（期望32字节，实际{}字节）", key.len());
                            panic!("无法从备份恢复密钥");
                        }
                        Err(e) => {
                            error!("读取备份密钥也失败: {}", e);
                            panic!("加密密钥文件读取失败且无有效备份");
                        }
                    }
                }
                panic!("读取加密密钥文件失败，请检查文件权限");
            }
        }
    }

    if Path::new(&key_backup_path).exists() {
        warn!("检测到备份密钥但主密钥不存在，正在从备份恢复...");
        match fs::read(&key_backup_path) {
            Ok(key) if key.len() == 32 => {
                info!("成功从备份恢复密钥!");
                if let Err(e) = fs::write(&key_path, &key) {
                    error!("恢复密钥后无法重新保存: {}", e);
                }
                return key;
            }
            Ok(_) => {
                warn!("备份密钥长度不正确，将生成新密钥");
            }
            Err(e) => {
                warn!("读取备份密钥失败: {}，将生成新密钥", e);
            }
        }
    }

    info!("加密密钥文件不存在，正在自动生成新密钥: {}", key_path);
    let mut key = vec![0u8; 32];
    rand::rng().fill(&mut key);

    if let Err(e) = fs::write(&key_path, &key) {
        error!("保存加密密钥失败: {}", e);
        panic!("保存加密密钥失败: {e}");
    }

    info!("加密密钥已生成并保存到: {}", key_path);
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
    info!("加密密钥完整性检查通过");
    info!("主密钥路径: {}", key_path);
    info!("备份密钥路径: {}", backup_path);

    if Path::new(&backup_path).exists() {
        match fs::read(&backup_path) {
            Ok(backup_key) if backup_key == key => {
                info!("密钥备份完整性检查通过（与主密钥一致）");
            }
            Ok(_) => {
                warn!("警告: 备份密钥与主密钥不一致!");
                warn!("这可能是正常的（如果主密钥是最近更新的）");
                warn!("也可能是数据损坏的信号（如果主密钥丢失后从备份恢复过）");
            }
            Err(e) => {
                warn!("警告: 无法读取备份密钥进行完整性检查: {}", e);
            }
        }
    } else {
        warn!("警告: 未找到密钥备份文件");
    }

    Ok(())
}

pub fn verify_key_with_sample(encrypted_sample: &str) -> bool {
    let decrypted = decrypt_password(encrypted_sample);
    !decrypted.is_empty()
}

#[must_use]
pub fn encrypt_password(password: &str) -> Option<String> {
    let key = get_encryption_key();
    let cipher = match Aes256Gcm::new_from_slice(&key) {
        Ok(c) => c,
        Err(e) => {
            error!("创建加密器失败: {}", e);
            return None;
        }
    };

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = match cipher.encrypt(nonce, password.as_bytes()) {
        Ok(ct) => ct,
        Err(e) => {
            error!("加密失败: {}", e);
            return None;
        }
    };

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);

    Some(BASE64.encode(&result))
}

#[must_use]
pub fn decrypt_password(encrypted_password: &str) -> String {
    let key = get_encryption_key();
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        error!("解密失败: 加密密钥长度不正确");
        return String::new();
    };

    let Ok(decoded) = BASE64.decode(encrypted_password) else {
        error!("解密失败: Base64解码错误");
        return String::new();
    };

    if decoded.len() < NONCE_SIZE {
        error!("解密失败: 密文长度不足 (期望>{}字节, 实际{}字节)", NONCE_SIZE, decoded.len());
        return String::new();
    }

    let (nonce_bytes, ciphertext) = decoded.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let Ok(plaintext) = cipher.decrypt(nonce, ciphertext) else {
        warn!("解密失败: AES-GCM解密错误");
        warn!("可能原因:");
        warn!("1. 数据库中的加密数据使用了旧密钥");
        warn!("2. 密钥文件(/etc/ipma/encryption.key)在程序运行后被修改或删除");
        warn!("3. 系统重启或容器重建导致密钥丢失");
        warn!("解决方案:");
        warn!("- 检查/etc/ipma/encryption.key.backup是否有旧密钥备份");
        warn!("- 如果有备份，尝试恢复到encryption.key");
        warn!("- 如果没有备份，受影响的加密数据(如SNMP密码、SMTP密码、2FA密钥)需要重新设置");
        warn!("密钥长度: {}字节", key.len());
        warn!("Nonce长度: {}字节", nonce_bytes.len());
        warn!("密文长度: {}字节", ciphertext.len());
        return String::new();
    };

    match String::from_utf8(plaintext.clone()) {
        Ok(s) => s,
        Err(_) => String::from_utf8_lossy(&plaintext).to_string()
    }
}

pub fn decrypt_credential(value: Option<&str>) -> Option<String> {
    value.map(decrypt_password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let password = "test_password_123";
        let encrypted = encrypt_password(password).unwrap();
        let decrypted = decrypt_password(&encrypted);
        assert_eq!(password, decrypted);
    }

    #[test]
    fn test_encrypt_produces_different_output() {
        let password = "test_password_123";
        let encrypted1 = encrypt_password(password).unwrap();
        let encrypted2 = encrypt_password(password).unwrap();
        assert_ne!(encrypted1, encrypted2);

        assert_eq!(password, decrypt_password(&encrypted1));
        assert_eq!(password, decrypt_password(&encrypted2));
    }
}
