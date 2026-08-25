use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Algorithm, Argon2, Params, Version,
};
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};

const KDF_MEMORY_KIB: u32 = 19_456;
const KDF_ITERATIONS: u32 = 2;
const KDF_PARALLELISM: u32 = 1;

#[derive(Debug)]
pub struct CryptoError(pub String);

impl std::fmt::Display for CryptoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Versioned envelope. Passwords are never included in the output or logs.
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct EncryptedExportEnvelope {
    format: String,
    version: u32,
    kdf: KdfEnvelope,
    cipher: CipherEnvelope,
    ciphertext: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct KdfEnvelope {
    algorithm: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CipherEnvelope {
    algorithm: String,
    nonce: String,
}

pub fn encrypt_export<T: Serialize>(snapshot: &T, password: &str) -> Result<Vec<u8>, CryptoError> {
    validate_password(password)?;
    let plaintext =
        serde_json::to_vec(snapshot).map_err(|_| CryptoError("无法生成加密导出数据".into()))?;
    let salt = SaltString::generate(&mut OsRng);
    let key = derive_key(password, salt.as_str())?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| CryptoError("无法初始化加密服务".into()))?;
    let mut nonce_bytes = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_ref())
        .map_err(|_| CryptoError("无法加密导出数据".into()))?;
    let envelope = EncryptedExportEnvelope {
        format: "mylist-encrypted-export".into(),
        version: 1,
        kdf: KdfEnvelope {
            algorithm: "argon2id".into(),
            memory_kib: KDF_MEMORY_KIB,
            iterations: KDF_ITERATIONS,
            parallelism: KDF_PARALLELISM,
            salt: salt.as_str().to_owned(),
        },
        cipher: CipherEnvelope {
            algorithm: "aes-256-gcm".into(),
            nonce: STANDARD_NO_PAD.encode(nonce_bytes),
        },
        ciphertext: STANDARD_NO_PAD.encode(ciphertext),
    };
    serde_json::to_vec_pretty(&envelope).map_err(|_| CryptoError("无法写入加密导出数据".into()))
}

pub fn is_encrypted_export(encoded: &[u8]) -> bool {
    serde_json::from_slice::<EncryptedExportEnvelope>(encoded)
        .map(|envelope| envelope.format == "mylist-encrypted-export")
        .unwrap_or(false)
}

pub fn decrypt_export(encoded: &[u8], password: &str) -> Result<Vec<u8>, CryptoError> {
    validate_password(password)?;
    let envelope: EncryptedExportEnvelope = serde_json::from_slice(encoded)
        .map_err(|_| CryptoError("加密文件格式无效或已损坏".into()))?;
    if envelope.format != "mylist-encrypted-export"
        || envelope.version != 1
        || envelope.kdf.algorithm != "argon2id"
        || envelope.kdf.memory_kib != KDF_MEMORY_KIB
        || envelope.kdf.iterations != KDF_ITERATIONS
        || envelope.kdf.parallelism != KDF_PARALLELISM
        || envelope.cipher.algorithm != "aes-256-gcm"
    {
        return Err(CryptoError("加密文件版本不受支持".into()));
    }
    let salt = SaltString::from_b64(&envelope.kdf.salt)
        .map_err(|_| CryptoError("加密文件格式无效或已损坏".into()))?;
    let key = derive_key(password, salt.as_str())?;
    let nonce = STANDARD_NO_PAD
        .decode(envelope.cipher.nonce)
        .map_err(|_| CryptoError("加密文件格式无效或已损坏".into()))?;
    if nonce.len() != 12 {
        return Err(CryptoError("加密文件格式无效或已损坏".into()));
    }
    let ciphertext = STANDARD_NO_PAD
        .decode(envelope.ciphertext)
        .map_err(|_| CryptoError("加密文件格式无效或已损坏".into()))?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| CryptoError("无法初始化解密服务".into()))?;
    cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| CryptoError("密码错误或文件校验失败".into()))
}

fn validate_password(password: &str) -> Result<(), CryptoError> {
    let password_length = password.chars().count();
    if !(6..=32).contains(&password_length) {
        return Err(CryptoError("密码需要 6 至 32 个字符".into()));
    }
    Ok(())
}

fn derive_key(password: &str, salt: &str) -> Result<[u8; 32], CryptoError> {
    let params = Params::new(KDF_MEMORY_KIB, KDF_ITERATIONS, KDF_PARALLELISM, Some(32))
        .map_err(|_| CryptoError("无法初始化密码派生参数".into()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0_u8; 32];
    argon
        .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut key)
        .map_err(|_| CryptoError("无法派生导出密钥".into()))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::{decrypt_export, encrypt_export, is_encrypted_export};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Sample<'a> {
        value: &'a str,
    }

    #[test]
    fn encrypted_export_round_trips_only_with_the_matching_password() {
        let encrypted = encrypt_export(
            &Sample {
                value: "本地数据"
            },
            "secure7",
        )
        .expect("encrypt");
        assert!(is_encrypted_export(&encrypted));
        let decoded = String::from_utf8(decrypt_export(&encrypted, "secure7").expect("decrypt"))
            .expect("utf8");
        assert_eq!(decoded, r#"{"value":"本地数据"}"#);
        assert!(decrypt_export(&encrypted, "wrong77").is_err());
    }

    #[test]
    fn encryption_rejects_passwords_outside_the_supported_range() {
        assert!(encrypt_export(&Sample { value: "x" }, "short").is_err());
        assert!(encrypt_export(&Sample { value: "x" }, &"a".repeat(33)).is_err());
    }
}
