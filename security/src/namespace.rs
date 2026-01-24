//! Namespace management for multi-tenancy

use crate::error::{Result, SecurityError};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Namespace (tenant)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    pub id: String,
    pub name: String,
    pub quota: NamespaceQuota,
    pub created_at: u64,
}

/// Namespace quota
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceQuota {
    /// Maximum storage in bytes
    pub max_storage: u64,
    /// Current storage usage
    pub current_storage: u64,
    /// Maximum number of tables
    pub max_tables: usize,
    /// Current number of tables
    pub current_tables: usize,
}

impl NamespaceQuota {
    /// Create new quota
    pub fn new(max_storage: u64, max_tables: usize) -> Self {
        Self {
            max_storage,
            current_storage: 0,
            max_tables,
            current_tables: 0,
        }
    }

    /// Check if can allocate storage
    pub fn can_allocate(&self, size: u64) -> bool {
        self.current_storage + size <= self.max_storage
    }

    /// Check if can create table
    pub fn can_create_table(&self) -> bool {
        self.current_tables < self.max_tables
    }

    /// Get usage percentage
    pub fn storage_usage_percentage(&self) -> f64 {
        (self.current_storage as f64 / self.max_storage as f64) * 100.0
    }
}

/// Namespace manager
pub struct NamespaceManager {
    namespaces: Arc<RwLock<HashMap<String, Namespace>>>,
    user_namespaces: Arc<RwLock<HashMap<String, String>>>, // user_id -> namespace_id
}

impl NamespaceManager {
    /// Create new namespace manager
    pub fn new() -> Self {
        Self {
            namespaces: Arc::new(RwLock::new(HashMap::new())),
            user_namespaces: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create namespace
    pub fn create_namespace(&mut self, name: &str, max_storage: u64, max_tables: usize) -> Result<Namespace> {
        let namespace_id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let namespace = Namespace {
            id: namespace_id.clone(),
            name: name.to_string(),
            quota: NamespaceQuota::new(max_storage, max_tables),
            created_at: now,
        };

        let mut namespaces = self.namespaces.write();
        namespaces.insert(namespace_id.clone(), namespace.clone());

        Ok(namespace)
    }

    /// Assign user to namespace
    pub fn assign_user_to_namespace(&mut self, user_id: &str, namespace_id: &str) -> Result<()> {
        // Check if namespace exists
        {
            let namespaces = self.namespaces.read();
            if !namespaces.contains_key(namespace_id) {
                return Err(SecurityError::NamespaceError(
                    "Namespace not found".to_string(),
                ));
            }
        }

        let mut user_ns = self.user_namespaces.write();
        user_ns.insert(user_id.to_string(), namespace_id.to_string());

        Ok(())
    }

    /// Get user's namespace
    pub fn get_user_namespace(&self, user_id: &str) -> Result<Namespace> {
        let user_ns = self.user_namespaces.read();
        let namespace_id = user_ns
            .get(user_id)
            .ok_or_else(|| SecurityError::NamespaceError("User not assigned to namespace".to_string()))?;

        let namespaces = self.namespaces.read();
        namespaces
            .get(namespace_id)
            .cloned()
            .ok_or_else(|| SecurityError::NamespaceError("Namespace not found".to_string()))
    }

    /// Update namespace storage usage
    pub fn update_storage_usage(&mut self, namespace_id: &str, delta: i64) -> Result<()> {
        let mut namespaces = self.namespaces.write();
        let namespace = namespaces
            .get_mut(namespace_id)
            .ok_or_else(|| SecurityError::NamespaceError("Namespace not found".to_string()))?;

        if delta >= 0 {
            namespace.quota.current_storage += delta as u64;
        } else {
            namespace.quota.current_storage = namespace.quota.current_storage.saturating_sub((-delta) as u64);
        }

        Ok(())
    }

    /// Get namespace count
    pub fn namespace_count(&self) -> usize {
        let namespaces = self.namespaces.read();
        namespaces.len()
    }
}

impl Default for NamespaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_quota() {
        let quota = NamespaceQuota::new(1024 * 1024, 10);
        
        assert!(quota.can_allocate(512 * 1024));
        assert!(quota.can_create_table());
        assert_eq!(quota.storage_usage_percentage(), 0.0);
    }

    #[test]
    fn test_create_namespace() {
        let mut manager = NamespaceManager::new();
        
        let ns = manager.create_namespace("tenant1", 1024 * 1024, 10).unwrap();
        
        assert_eq!(ns.name, "tenant1");
        assert_eq!(manager.namespace_count(), 1);
    }

    #[test]
    fn test_assign_user_to_namespace() {
        let mut manager = NamespaceManager::new();
        
        let ns = manager.create_namespace("tenant1", 1024 * 1024, 10).unwrap();
        manager.assign_user_to_namespace("user1", &ns.id).unwrap();
        
        let user_ns = manager.get_user_namespace("user1").unwrap();
        assert_eq!(user_ns.id, ns.id);
    }

    #[test]
    fn test_update_storage_usage() {
        let mut manager = NamespaceManager::new();
        
        let ns = manager.create_namespace("tenant1", 1024 * 1024, 10).unwrap();
        
        manager.update_storage_usage(&ns.id, 512 * 1024).unwrap();
        
        let updated_ns = manager.namespaces.read().get(&ns.id).cloned().unwrap();
        assert_eq!(updated_ns.quota.current_storage, 512 * 1024);
    }
}
