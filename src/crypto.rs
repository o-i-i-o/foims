use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use rand::RngExt;
use std::fs;
use std::path::Path;
use tracing::info;

const NONCE_SIZE: usize = 12;

pub fn get_encryption_key() -> Vec<u8> {
    let app_name = "ipma";
    let key_path = format!("/etc/{}/encryption.key", app_name);

    let key_dir = Path::new(&key_path).parent().unwrap();
    if !key_dir.exists() {
        fs::create_dir_all(key_dir).unwrap_or_else(|e| {
            info!("创建加密密钥目录失败: {}", e);
        });
    }

    if Path::new(&key_path).exists()
        && let Ok(key) = fs::read(&key_path)
        && key.len() == 32
    {
        return key;
    }

    let mut key = vec![0u8; 32];
    rand::rng().fill(&mut key);

    if let Err(e) = fs::write(&key_path, &key) {
        info!("保存加密密钥失败: {}", e);
    }

    key
}

pub fn encrypt_password(password: &str) -> String {
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("无效的密钥长度");

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, password.as_bytes())
        .expect("加密失败");

    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);

    BASE64.encode(&result)
}

pub fn decrypt_password(encrypted_password: &str) -> String {
    let key = get_encryption_key();
    let cipher = Aes256Gcm::new_from_slice(&key).expect("无效的密钥长度");

    let decoded = match BASE64.decode(encrypted_password) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };

    if decoded.len() < NONCE_SIZE {
        return String::new();
    }

    let (nonce_bytes, ciphertext) = decoded.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = match cipher.decrypt(nonce, ciphertext) {
        Ok(p) => p,
        Err(_) => return String::new(),
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
