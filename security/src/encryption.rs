//! Encryption at-rest using AES-256

use crate::error::{Result, SecurityError};
use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, AES_256_GCM};
use ring::error::Unspecified;
use ring::rand::{SecureRandom, SystemRandom};

/// Nonce sequence for encryption
struct CounterNonceSequence(u64);

impl NonceSequence for CounterNonceSequence {
    fn advance(&mut self) -> core::result::Result<Nonce, Unspecified> {
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..].copy_from_slice(&self.0.to_le_bytes());
        self.0 = self.0.wrapping_add(1);
        Nonce::try_assume_unique_for_key(&nonce_bytes)
    }
}

/// Encryption manager
pub struct EncryptionManager {
    rng: SystemRandom,
}

impl EncryptionManager {
    /// Create new encryption manager
    pub fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
        }
    }

    /// Encrypt data using AES-256-GCM
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Generate random key (in production, use KMS)
        let mut key_bytes = [0u8; 32];
        self.rng
            .fill(&mut key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Failed to generate key".to_string()))?;

        let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Failed to create key".to_string()))?;

        let nonce_sequence = CounterNonceSequence(0);
        let mut sealing_key = SealingKey::new(unbound_key, nonce_sequence);

        let mut in_out = data.to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut in_out)
            .map_err(|_| SecurityError::EncryptionError("Encryption failed".to_string()))?;

        // Prepend key (in production, store in KMS)
        let mut result = key_bytes.to_vec();
        result.extend_from_slice(&in_out);

        Ok(result)
    }

    /// Decrypt data
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 32 {
            return Err(SecurityError::EncryptionError(
                "Invalid encrypted data".to_string(),
            ));
        }

        // Extract key (in production, get from KMS)
        let key_bytes = &data[..32];
        let encrypted_data = &data[32..];

        let unbound_key = UnboundKey::new(&AES_256_GCM, key_bytes)
            .map_err(|_| SecurityError::EncryptionError("Failed to create key".to_string()))?;

        let nonce_sequence = CounterNonceSequence(0);
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
        
        assert_ne!(encrypted, data);
        assert!(encrypted.len() > data.len()); // Includes key and tag
        
        let decrypted = manager.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, data);
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
        
        let invalid_data = b"invalid";
        let result = manager.decrypt(invalid_data);
        
        assert!(result.is_err());
    }
}
