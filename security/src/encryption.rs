//! Encryption at-rest using AES-256-GCM
//!
//! The encryption key is stored in the EncryptionManager and is NOT
//! included in the ciphertext output. Only the random nonce is prepended.
//! Format: [12-byte nonce][ciphertext + 16-byte GCM tag]

use crate::error::{Result, SecurityError};
use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, AES_256_GCM};
use ring::error::Unspecified;
use ring::rand::{SecureRandom, SystemRandom};

const NONCE_LEN: usize = 12;

/// Nonce sequence that yields a single predetermined nonce.
struct SingleNonceSequence(Option<[u8; NONCE_LEN]>);

impl NonceSequence for SingleNonceSequence {
    fn advance(&mut self) -> core::result::Result<Nonce, Unspecified> {
        let bytes = self.0.take().ok_or(Unspecified)?;
        Nonce::try_assume_unique_for_key(&bytes)
    }
}

/// Encryption manager that holds a persistent AES-256-GCM key.
pub struct EncryptionManager {
    key_bytes: [u8; 32],
    rng: SystemRandom,
}

impl EncryptionManager {
    /// Create new encryption manager with a randomly generated key.
    pub fn new() -> Self {
        let rng = SystemRandom::new();
        let mut key_bytes = [0u8; 32];
        rng.fill(&mut key_bytes).expect("Failed to generate encryption key");
        Self { key_bytes, rng }
    }

    /// Create encryption manager with a specific key (e.g. from KMS).
    pub fn with_key(key_bytes: [u8; 32]) -> Self {
        Self {
            key_bytes,
            rng: SystemRandom::new(),
        }
    }

    /// Encrypt data using AES-256-GCM.
    /// Output format: [12-byte nonce][ciphertext + 16-byte tag]
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|_| SecurityError::EncryptionError("Failed to generate nonce".to_string()))?;

        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Failed to create key".to_string()))?;

        let nonce_sequence = SingleNonceSequence(Some(nonce_bytes));
        let mut sealing_key = SealingKey::new(unbound_key, nonce_sequence);

        let mut in_out = data.to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut in_out)
            .map_err(|_| SecurityError::EncryptionError("Encryption failed".to_string()))?;

        // Prepend nonce (NOT the key) to ciphertext
        let mut result = Vec::with_capacity(NONCE_LEN + in_out.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&in_out);

        Ok(result)
    }

    /// Decrypt data encrypted with this manager's key.
    /// Input format: [12-byte nonce][ciphertext + 16-byte tag]
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < NONCE_LEN {
            return Err(SecurityError::EncryptionError(
                "Invalid encrypted data: too short".to_string(),
            ));
        }

        let mut nonce_bytes = [0u8; NONCE_LEN];
        nonce_bytes.copy_from_slice(&data[..NONCE_LEN]);
        let encrypted_data = &data[NONCE_LEN..];

        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Failed to create key".to_string()))?;

        let nonce_sequence = SingleNonceSequence(Some(nonce_bytes));
        let mut opening_key = OpeningKey::new(unbound_key, nonce_sequence);

        let mut in_out = encrypted_data.to_vec();
        let decrypted = opening_key
            .open_in_place(Aad::empty(), &mut in_out)
            .map_err(|_| SecurityError::EncryptionError("Decryption failed".to_string()))?;

        Ok(decrypted.to_vec())
    }
}

impl Default for EncryptionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let manager = EncryptionManager::new();

        let data = b"sensitive data";
        let encrypted = manager.encrypt(data).unwrap();

        assert_ne!(&encrypted[..], &data[..]);
        // Output: 12 byte nonce + data len + 16 byte tag
        assert_eq!(encrypted.len(), NONCE_LEN + data.len() + 16);

        let decrypted = manager.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_encrypt_decrypt_with_explicit_key() {
        let key = [42u8; 32];
        let manager = EncryptionManager::with_key(key);

        let data = b"secret message";
        let encrypted = manager.encrypt(data).unwrap();
        let decrypted = manager.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_different_manager_cannot_decrypt() {
        let manager1 = EncryptionManager::new();
        let manager2 = EncryptionManager::new();

        let data = b"sensitive data";
        let encrypted = manager1.encrypt(data).unwrap();

        // A different manager (different key) should fail to decrypt
        let result = manager2.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_empty() {
        let manager = EncryptionManager::new();

        let data = b"";
        let encrypted = manager.encrypt(data).unwrap();
        let decrypted = manager.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_decrypt_invalid() {
        let manager = EncryptionManager::new();

        let invalid_data = b"short";
        let result = manager.decrypt(invalid_data);

        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let manager = EncryptionManager::new();

        let data = b"sensitive data";
        let mut encrypted = manager.encrypt(data).unwrap();

        // Tamper with the ciphertext (flip a bit after the nonce)
        if encrypted.len() > NONCE_LEN {
            encrypted[NONCE_LEN] ^= 0xFF;
        }

        let result = manager.decrypt(&encrypted);
        assert!(result.is_err());
    }
}
