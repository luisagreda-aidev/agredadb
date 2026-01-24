//! Role-Based Access Control (RBAC)

use crate::error::{Result, SecurityError};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// Administrator (full access)
    Admin,
    /// Read and write access
    ReadWrite,
    /// Read-only access
    ReadOnly,
    /// Custom role
    Custom,
}

/// Permission
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Read permission
    Read,
    /// Write permission
    Write,
    /// Delete permission
    Delete,
    /// Admin permission
    Admin,
}

impl Role {
    /// Get permissions for role
    pub fn permissions(&self) -> HashSet<Permission> {
        match self {
            Self::Admin => {
                vec![
                    Permission::Read,
                    Permission::Write,
                    Permission::Delete,
                    Permission::Admin,
                ]
                .into_iter()
                .collect()
            }
            Self::ReadWrite => {
                vec![Permission::Read, Permission::Write]
                    .into_iter()
                    .collect()
            }
            Self::ReadOnly => {
                vec![Permission::Read].into_iter().collect()
            }
            Self::Custom => HashSet::new(),
        }
    }
}

/// RBAC manager
pub struct RbacManager {
    /// User roles
    user_roles: Arc<RwLock<HashMap<String, Role>>>,
    /// Custom permissions (for Custom role)
    custom_permissions: Arc<RwLock<HashMap<String, HashSet<Permission>>>>,
    /// Resource permissions (resource -> required permission)
    resource_permissions: Arc<RwLock<HashMap<String, Permission>>>,
}

impl RbacManager {
    /// Create new RBAC manager
    pub fn new() -> Self {
        Self {
            user_roles: Arc::new(RwLock::new(HashMap::new())),
            custom_permissions: Arc::new(RwLock::new(HashMap::new())),
            resource_permissions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Assign role to user
    pub async fn assign_role(&mut self, user_id: &str, role: Role) -> Result<()> {
        let mut roles = self.user_roles.write();
        roles.insert(user_id.to_string(), role);
        Ok(())
    }

    /// Get user role
    pub fn get_role(&self, user_id: &str) -> Option<Role> {
        let roles = self.user_roles.read();
        roles.get(user_id).copied()
    }

    /// Add custom permission
    pub fn add_custom_permission(&mut self, user_id: &str, permission: Permission) {
        let mut perms = self.custom_permissions.write();
        perms
            .entry(user_id.to_string())
            .or_insert_with(HashSet::new)
            .insert(permission);
    }

    /// Check if user has permission
    pub async fn check_permission(
        &self,
        user_id: &str,
        permission: Permission,
        _resource: &str,
    ) -> Result<bool> {
        // Get user role
        let role = self.get_role(user_id)
            .ok_or_else(|| SecurityError::UserNotFound(user_id.to_string()))?;

        // Check role permissions
        if role.permissions().contains(&permission) {
            return Ok(true);
        }

        // Check custom permissions
        let custom_perms = self.custom_permissions.read();
        if let Some(perms) = custom_perms.get(user_id) {
            if perms.contains(&permission) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Set resource permission requirement
    pub fn set_resource_permission(&mut self, resource: &str, permission: Permission) {
        let mut res_perms = self.resource_permissions.write();
        res_perms.insert(resource.to_string(), permission);
    }
}

impl Default for RbacManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_role_permissions() {
        assert_eq!(Role::Admin.permissions().len(), 4);
        assert_eq!(Role::ReadWrite.permissions().len(), 2);
        assert_eq!(Role::ReadOnly.permissions().len(), 1);
    }

    #[tokio::test]
    async fn test_assign_role() {
        let mut rbac = RbacManager::new();
        
        rbac.assign_role("user1", Role::Admin).await.unwrap();
        
        let role = rbac.get_role("user1").unwrap();
        assert_eq!(role, Role::Admin);
    }

    #[tokio::test]
    async fn test_check_permission() {
        let mut rbac = RbacManager::new();
        
        rbac.assign_role("user1", Role::ReadOnly).await.unwrap();
        
        let can_read = rbac.check_permission("user1", Permission::Read, "table1")
            .await
            .unwrap();
        assert!(can_read);
        
        let can_write = rbac.check_permission("user1", Permission::Write, "table1")
            .await
            .unwrap();
        assert!(!can_write);
    }

    #[tokio::test]
    async fn test_custom_permissions() {
        let mut rbac = RbacManager::new();
        
        rbac.assign_role("user1", Role::Custom).await.unwrap();
        rbac.add_custom_permission("user1", Permission::Read);
        
        let can_read = rbac.check_permission("user1", Permission::Read, "table1")
            .await
            .unwrap();
        assert!(can_read);
    }
}
