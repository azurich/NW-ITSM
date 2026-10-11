//! Values encrypted by ITSM-NG with `config/glpicrypt.key`.
//!
//! Same format as `Toolbox::sodiumEncrypt`: XChaCha20-Poly1305 with a
//! random 24-byte nonce, also used as associated data, and
//! `base64(nonce || ciphertext)`. Used for LDAP bind passwords, mail
//! collector passwords and the SMTP / proxy passwords of the configuration.

use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    AeadCore, KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, OsRng, Payload},
};

const NONCE_LEN: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    /// The key is not 32 bytes long.
    InvalidKey,
    /// Not base64, too short, or encrypted with another key.
    InvalidCiphertext,
    /// The decrypted value is not UTF-8.
    InvalidText,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidKey => "the encryption key must be 32 bytes long",
            Self::InvalidCiphertext => "the value cannot be decrypted with this key",
            Self::InvalidText => "the decrypted value is not valid UTF-8",
        })
    }
}

impl std::error::Error for CryptoError {}

/// The content of `glpicrypt.key` (32 raw bytes).
#[derive(Clone)]
pub struct GlpiKey(XChaCha20Poly1305);

impl std::fmt::Debug for GlpiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GlpiKey(..)")
    }
}

impl GlpiKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        XChaCha20Poly1305::new_from_slice(bytes)
            .map(Self)
            .map_err(|_| CryptoError::InvalidKey)
    }

    /// Decrypts a stored value. Like the PHP code, an empty value stays empty.
    pub fn decrypt(&self, value: &str) -> Result<String, CryptoError> {
        if value.is_empty() {
            return Ok(String::new());
        }
        let bytes = STANDARD
            .decode(value)
            .map_err(|_| CryptoError::InvalidCiphertext)?;
        if bytes.len() < NONCE_LEN {
            return Err(CryptoError::InvalidCiphertext);
        }
        let (nonce, ciphertext) = bytes.split_at(NONCE_LEN);
        let plaintext = self
            .0
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: nonce,
                },
            )
            .map_err(|_| CryptoError::InvalidCiphertext)?;
        String::from_utf8(plaintext).map_err(|_| CryptoError::InvalidText)
    }

    /// Encrypts a value so that the PHP application can decrypt it.
    pub fn encrypt(&self, value: &str) -> String {
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = self
            .0
            .encrypt(
                &nonce,
                Payload {
                    msg: value.as_bytes(),
                    aad: &nonce,
                },
            )
            .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory data");
        let mut bytes = nonce.to_vec();
        bytes.extend(ciphertext);
        STANDARD.encode(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> GlpiKey {
        GlpiKey::from_bytes(&[b'k'; 32]).unwrap()
    }

    #[test]
    fn decrypts_values_encrypted_by_php() {
        // Toolbox::sodiumEncrypt('s3cr3t-pässwörd') with the key 'k' x 32
        // and the nonce 'n' x 24.
        let encrypted =
            "bm5ubm5ubm5ubm5ubm5ubm5ubm5ubm5uhsurx/UsArAgEGwLpPsTh26SiFHrvNbLlqw9A79K4XvV";
        assert_eq!(key().decrypt(encrypted).unwrap(), "s3cr3t-pässwörd");
    }

    #[test]
    fn round_trip_with_random_nonces() {
        let first = key().encrypt("secret");
        let second = key().encrypt("secret");
        assert_ne!(first, second);
        assert_eq!(key().decrypt(&first).unwrap(), "secret");
    }

    #[test]
    fn empty_values_stay_empty() {
        assert_eq!(key().decrypt("").unwrap(), "");
    }

    #[test]
    fn wrong_key_or_garbage_is_rejected() {
        let encrypted = key().encrypt("secret");
        let other = GlpiKey::from_bytes(&[b'o'; 32]).unwrap();
        assert_eq!(
            other.decrypt(&encrypted),
            Err(CryptoError::InvalidCiphertext)
        );
        assert_eq!(
            key().decrypt("not base64!"),
            Err(CryptoError::InvalidCiphertext)
        );
        assert_eq!(
            key().decrypt("c2hvcnQ="),
            Err(CryptoError::InvalidCiphertext)
        );
        assert_eq!(
            GlpiKey::from_bytes(b"short").unwrap_err(),
            CryptoError::InvalidKey
        );
    }
}
