//! Authentication management

use crate::error::{Result, SecurityError};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// User
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password_hash: String,
    pub tenant: String,
    pub created_at: u64,
}

/// Credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

/// JWT Claims (public for verification)
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // User ID
    pub username: String,
    pub tenant: String,
    pub exp: u64,         // Expiration time
    pub iat: u64,         // Issued at
}

/// Authentication manager
pub struct AuthManager {
    users: Arc<RwLock<HashMap<String, User>>>,
    username_to_id: Arc<RwLock<HashMap<String, String>>>,
    secret_key: String,
}

impl AuthManager {
    /// Create new auth manager
    pub fn new(secret_key: &str) -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            username_to_id: Arc::new(RwLock::new(HashMap::new())),
            secret_key: secret_key.to_string(),
        }
    }

    /// Create user
    pub async fn create_user(&self, username: &str, password: &str, tenant: &str) -> Result<User> {
        // Check if user exists
        {
            let username_map = self.username_to_id.read();
            if username_map.contains_key(username) {
                return Err(SecurityError::AuthenticationFailed(
                    "User already exists".to_string(),
                ));
            }
        }

        // Hash password
        let password_hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
            .map_err(|e| SecurityError::EncryptionError(e.to_string()))?;

        // Create user
        let user_id = uuid::Uuid::new_v4().to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let user = User {
            id: user_id.clone(),
            username: username.to_string(),
            password_hash,
            tenant: tenant.to_string(),
            created_at: now,
        };

        // Store user
        {
            let mut users = self.users.write();
            users.insert(user_id.clone(), user.clone());
        }

        {
            let mut username_map = self.username_to_id.write();
            username_map.insert(username.to_string(), user_id);
        }

        Ok(user)
    }

    /// Authenticate user and return JWT token
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<String> {
        // Get user ID
        let user_id = {
            let username_map = self.username_to_id.read();
            username_map
                .get(username)
                .cloned()
                .ok_or(SecurityError::InvalidCredentials)?
        };

        // Get user
        let user = {
            let users = self.users.read();
            users
                .get(&user_id)
                .cloned()
                .ok_or(SecurityError::UserNotFound(username.to_string()))?
        };

        // Verify password
        let valid = bcrypt::verify(password, &user.password_hash)
            .map_err(|e| SecurityError::EncryptionError(e.to_string()))?;

        if !valid {
            return Err(SecurityError::InvalidCredentials);
        }

        // Generate JWT token
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: user.id.clone(),
            username: user.username.clone(),
            tenant: user.tenant.clone(),
            exp: now + 3600, // 1 hour expiration
            iat: now,
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret_key.as_bytes()),
        )
        .map_err(|e| SecurityError::InvalidToken(e.to_string()))?;

        Ok(token)
    }

    /// Verify JWT token
    pub fn verify_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret_key.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| SecurityError::InvalidToken(e.to_string()))?;

        Ok(token_data.claims)
    }

    /// Get user by ID
    pub fn get_user(&self, user_id: &str) -> Result<User> {
        let users = self.users.read();
        users
            .get(user_id)
            .cloned()
            .ok_or_else(|| SecurityError::UserNotFound(user_id.to_string()))
    }

    /// Get user count
    pub fn user_count(&self) -> usize {
        let users = self.users.read();
        users.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_user() {
        let auth = AuthManager::new("test_secret");
        
        let user = auth.create_user("alice", "password123", "tenant1")
            .await
            .unwrap();
        
        assert_eq!(user.username, "alice");
        assert_eq!(user.tenant, "tenant1");
        assert_ne!(user.password_hash, "password123"); // Should be hashed
    }

    #[tokio::test]
    async fn test_authenticate() {
        let auth = AuthManager::new("test_secret");
        
        auth.create_user("bob", "secret456", "tenant1")
            .await
            .unwrap();
        
        let token = auth.authenticate("bob", "secret456")
            .await
            .unwrap();
        
        assert!(!token.is_empty());
    }

    #[tokio::test]
    async fn test_authenticate_wrong_password() {
        let auth = AuthManager::new("test_secret");
        
        auth.create_user("charlie", "correct", "tenant1")
            .await
            .unwrap();
        
        let result = auth.authenticate("charlie", "wrong").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_verify_token() {
        let auth = AuthManager::new("test_secret");
        
        auth.create_user("dave", "password", "tenant1")
            .await
            .unwrap();
        
        let token = auth.authenticate("dave", "password")
            .await
            .unwrap();
        
        let claims = auth.verify_token(&token).unwrap();
        assert_eq!(claims.username, "dave");
        assert_eq!(claims.tenant, "tenant1");
    }
}
