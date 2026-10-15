//! Local key stays outside the database. AEAD binds ciphertext to its record.
use base64::{engine::general_purpose::STANDARD, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde_json::{json, Value};
use sha2::Sha256;

#[derive(Clone)]
pub struct Vault {
    key: [u8; 32],
}
impl Vault {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::symlink_metadata(path)
            .map_err(|_| "key file missing; run jarvis-cli init".to_string())?;
        if !meta.is_file() || meta.mode() & 0o077 != 0 || meta.uid() != unsafe { libc::geteuid() } {
            return Err("key file must be owned by this user and mode 0600".into());
        }
        let bytes = std::fs::read(path).map_err(|_| "cannot read key")?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "key must contain 32 raw bytes")?;
        Ok(Self { key })
    }
    #[cfg(test)]
    pub fn test() -> Self {
        Self { key: [42; 32] }
    }
    pub fn seal(&self, id: &str, body: &Value) -> Result<Value, String> {
        let mut nonce = [0; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let bytes = serde_json::to_vec(body).map_err(|_| "invalid payload")?;
        let encrypted = ChaCha20Poly1305::new((&self.key).into())
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &bytes,
                    aad: id.as_bytes(),
                },
            )
            .map_err(|_| "encryption failed")?;
        Ok(json!({"v":1,"nonce":STANDARD.encode(nonce),"ciphertext":STANDARD.encode(encrypted)}))
    }
    pub fn open(&self, id: &str, sealed: &Value) -> Result<Value, String> {
        if sealed["v"] != 1 {
            return Err("unknown encryption version".into());
        }
        let nonce = STANDARD
            .decode(sealed["nonce"].as_str().ok_or("missing nonce")?)
            .map_err(|_| "invalid nonce")?;
        if nonce.len() != 12 {
            return Err("invalid nonce length".into());
        }
        let bytes = STANDARD
            .decode(sealed["ciphertext"].as_str().ok_or("missing ciphertext")?)
            .map_err(|_| "invalid ciphertext")?;
        let plain = ChaCha20Poly1305::new((&self.key).into())
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &bytes,
                    aad: id.as_bytes(),
                },
            )
            .map_err(|_| "key or payload authentication failed")?;
        serde_json::from_slice(&plain).map_err(|_| "invalid decrypted JSON".into())
    }
    pub fn key_hash(&self, key: &str) -> String {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.key).expect("valid key");
        mac.update(b"jarvis-memory-key-v1\0owner\0");
        mac.update(key.as_bytes());
        format!("{:x}", mac.finalize().into_bytes())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn payload_is_bound_to_record_and_key() {
        let v = Vault::test();
        let body = json!({"secret":"private"});
        let sealed = v.seal("turn:a", &body).unwrap();
        assert!(!sealed.to_string().contains("private"));
        assert_eq!(v.open("turn:a", &sealed).unwrap(), body);
        assert!(v.open("turn:b", &sealed).is_err());
        assert!(Vault { key: [1; 32] }.open("turn:a", &sealed).is_err());
    }
}
