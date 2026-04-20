use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use rand::RngExt;
use std::fs;
use std::path::Path;
use tracing::{error, info};

const NONCE_SIZE: usize = 12;

pub fn get_encryption_key() -> Vec<u8> {
    let app_name = "ipma";
    let key_path = format!("/etc/{app_name}/encryption.key");

    let Some(key_dir) = Path::new(&key_path).parent() else {
        error!("无法获取加密密钥目录的父目录");
        panic!("无法获取加密密钥目录的父目录");
    };
    if !key_dir.exists()
        && let Err(e) = fs::create_dir_all(key_dir)
    {
        error!("创建加密密钥目录失败: {}", e);
        panic!("创建加密密钥目录失败: {e}");
    }

    if Path::new(&key_path).exists() {
        if let Ok(key) = fs::read(&key_path) {
            if key.len() == 32 {
                return key;
            }
            error!(
                "加密密钥文件长度不正确（期望32字节，实际{}字节），请重新生成密钥",
                key.len()
            );
            panic!(
                "加密密钥文件长度不正确（期望32字节，实际{}字节）",
                key.len()
            );
        }
        error!("读取加密密钥文件失败，请检查文件权限");
        panic!("读取加密密钥文件失败，请检查文件权限");
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

#[must_use] 
pub fn encrypt_password(password: &str) -> Option<String> {
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).ok()?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, password.as_bytes()).ok()?;

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);

    Some(BASE64.encode(&result))
}

#[must_use] 
pub fn decrypt_password(encrypted_password: &str) -> String {
    let key = get_encryption_key();
    let Ok(cipher) = Aes256Gcm::new_from_slice(&key) else {
        return String::new();
    };

    let Ok(decoded) = BASE64.decode(encrypted_password) else {
        return String::new();
    };

    if decoded.len() < NONCE_SIZE {
        return String::new();
    }

    let (nonce_bytes, ciphertext) = decoded.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let Ok(plaintext) = cipher.decrypt(nonce, ciphertext) else {
        return String::new();
    };

    String::from_utf8_lossy(&plaintext).to_string()
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
        let encrypted = encrypt_password(password);
        let decrypted = decrypt_password(&encrypted);
        assert_eq!(password, decrypted);
    }

    #[test]
    fn test_encrypt_produces_different_output() {
        let password = "test_password_123";
        let encrypted1 = encrypt_password(password);
        let encrypted2 = encrypt_password(password);
        assert_ne!(encrypted1, encrypted2);

        assert_eq!(password, decrypt_password(&encrypted1));
        assert_eq!(password, decrypt_password(&encrypted2));
    }
}
