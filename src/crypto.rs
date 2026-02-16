use aes::Aes256;
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyInit, generic_array::GenericArray};
use rand::RngExt;
use std::fs;
use std::path::Path;
use tracing::info;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

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
    let key = GenericArray::from_slice(&key);
    let mut cipher = Aes256::new(key);

    let mut plaintext = password.as_bytes().to_vec();
    let padding = 16 - (plaintext.len() % 16);
    plaintext.extend(vec![padding as u8; padding]);

    let mut ciphertext = plaintext;
    for chunk in ciphertext.chunks_mut(16) {
        let block = GenericArray::from_mut_slice(chunk);
        cipher.encrypt_block_mut(block);
    }

    BASE64.encode(&ciphertext)
}

pub fn decrypt_password(encrypted_password: &str) -> String {
    let key = get_encryption_key();
    let key = GenericArray::from_slice(&key);
    let mut cipher = Aes256::new(key);

    let mut ciphertext = BASE64.decode(encrypted_password).unwrap_or_default();

    for chunk in ciphertext.chunks_mut(16) {
        let block = GenericArray::from_mut_slice(chunk);
        cipher.decrypt_block_mut(block);
    }

    let padding = ciphertext.last().copied().unwrap_or(0) as usize;
    if padding > 0 && padding <= 16 {
        ciphertext.truncate(ciphertext.len() - padding);
    }

    String::from_utf8_lossy(&ciphertext).to_string()
}
