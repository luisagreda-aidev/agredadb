//! # AgredaDB Security & Multi-tenancy
//!
//! Enterprise-grade security with multi-tenant isolation.
//!
//! ## Features
//!
//! - **Authentication**: JWT, OAuth2, API keys
//! - **Authorization**: Role-Based Access Control (RBAC)
//! - **Encryption**: At-rest (AES-256) and in-transit (TLS)
//! - **Multi-tenancy**: Namespace isolation
//! - **Audit Logging**: Complete audit trail
//!
//! ## Example
//!
//! ```rust
//! use agredadb_security::{SecurityManager, User, Role, Permission};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create security manager
//! let mut security = SecurityManager::new("secret_key");
//!
//! // Create user
//! let user = security.create_user("alice", "password123", "tenant1").await?;
//!
//! // Assign role
//! security.assign_role(&user.id, Role::Admin).await?;
//!
//! // Check permission
//! let can_write = security.check_permission(&user.id, Permission::Write, "table1").await?;
//! # Ok(())
//! # }
//! ```

pub mod auth;
pub mod rbac;
pub mod encryption;
pub mod namespace;
pub mod audit;
pub mod error;

pub use auth::{AuthManager, User, Credentials, Claims};
pub use rbac::{RbacManager, Role, Permission};
pub use encryption::EncryptionManager;
pub use namespace::{NamespaceManager, Namespace};
pub use audit::{AuditLogger, AuditEvent};
pub use error::{SecurityError, Result};

/// Security manager (combines all security features)
pub struct SecurityManager {
    auth: AuthManager,
    rbac: RbacManager,
    encryption: EncryptionManager,
    _namespace: NamespaceManager,
    audit: AuditLogger,
}

impl SecurityManager {
    /// Create new security manager
    pub fn new(secret_key: &str) -> Self {
        Self {
            auth: AuthManager::new(secret_key),
            rbac: RbacManager::new(),
            encryption: EncryptionManager::new(),
            _namespace: NamespaceManager::new(),
            audit: AuditLogger::new(),
        }
    }

    /// Create user
    pub async fn create_user(&self, username: &str, password: &str, tenant: &str) -> Result<User> {
        let user = self.auth.create_user(username, password, tenant).await?;
        self.audit.log(AuditEvent::UserCreated {
            user_id: user.id.clone(),
            username: username.to_string(),
            tenant: tenant.to_string(),
        });
        Ok(user)
    }

    /// Authenticate user
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<String> {
        let token = self.auth.authenticate(username, password).await?;
        self.audit.log(AuditEvent::UserAuthenticated {
            username: username.to_string(),
        });
        Ok(token)
    }

    /// Assign role
    pub async fn assign_role(&mut self, user_id: &str, role: Role) -> Result<()> {
        self.rbac.assign_role(user_id, role).await?;
        self.audit.log(AuditEvent::RoleAssigned {
            user_id: user_id.to_string(),
            role: format!("{:?}", role),
        });
        Ok(())
    }

    /// Check permission
    pub async fn check_permission(&self, user_id: &str, permission: Permission, resource: &str) -> Result<bool> {
        self.rbac.check_permission(user_id, permission, resource).await
    }

    /// Encrypt data
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.encryption.encrypt(data)
    }

    /// Decrypt data
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.encryption.decrypt(data)
    }

    /// Verify JWT token and return claims
    pub fn verify_token(&self, token: &str) -> Result<auth::Claims> {
        self.auth.verify_token(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_security_manager() {
        let security = SecurityManager::new("test_secret");
        
        // Create user
        let user = security.create_user("alice", "password123", "tenant1")
            .await
            .unwrap();
        
        assert_eq!(user.username, "alice");
        assert_eq!(user.tenant, "tenant1");
    }

    #[tokio::test]
    async fn test_authentication_flow() {
        let security = SecurityManager::new("test_secret");
        
        // Create user
        security.create_user("bob", "secret456", "tenant1")
            .await
            .unwrap();
        
        // Authenticate
        let token = security.authenticate("bob", "secret456")
            .await
            .unwrap();
        
        assert!(!token.is_empty());
    }
}
