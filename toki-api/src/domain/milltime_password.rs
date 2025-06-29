use aes_gcm::{
    aead::{generic_array::GenericArray, Aead, OsRng},
    AeadCore, Aes256Gcm, Key, KeyInit,
};
use base64::prelude::*;
use std::env;

pub struct MilltimePassword(String);

impl MilltimePassword {
    pub fn new(password: String) -> Self {
        Self(password)
    }

    pub fn from_encrypted(encrypted: String) -> Self {
        let (cipher_b64, nonce_b64) = encrypted.split_once(':').unwrap();
        let cipher_bytes = BASE64_STANDARD.decode(cipher_b64).unwrap();
        let nonce_bytes = BASE64_STANDARD.decode(nonce_b64).unwrap();

        let env_key = key_bytes_from_env();
        let key = Key::<Aes256Gcm>::from_slice(&env_key);

        let cipher = Aes256Gcm::new(key);
        let nonce = GenericArray::from_slice(nonce_bytes.as_ref());
        let decrypted = cipher.decrypt(nonce, cipher_bytes.as_ref()).unwrap();

        Self(String::from_utf8(decrypted).unwrap())
    }

    pub fn to_encrypted(&self) -> String {
        let env_key = key_bytes_from_env();
        let key = Key::<Aes256Gcm>::from_slice(&env_key);

        let cipher = Aes256Gcm::new(key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits; unique per message
        let ciphertext = cipher.encrypt(&nonce, self.0.as_bytes()).unwrap();

        let cipher_b64 = BASE64_STANDARD.encode(ciphertext);
        let nonce_b64 = BASE64_STANDARD.encode(nonce);
        format!("{cipher_b64}:{nonce_b64}")
    }
}

impl AsRef<str> for MilltimePassword {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn key_bytes_from_env() -> Vec<u8> {
    let key_b64 = env::var("MT_CRYPTO_KEY").unwrap();
    BASE64_STANDARD.decode(key_b64).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption() {
        dotenvy::from_path(".env.local").ok();

        let password = MilltimePassword::new("passwordX".to_string());
        let encrypted = password.to_encrypted();
        let decrypted = MilltimePassword::from_encrypted(encrypted);
        assert_eq!(decrypted.as_ref(), "passwordX");
    }
}
