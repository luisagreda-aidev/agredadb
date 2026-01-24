//! Audit logging

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditEvent {
    /// User created
    UserCreated {
        user_id: String,
        username: String,
        tenant: String,
    },
    /// User authenticated
    UserAuthenticated {
        username: String,
    },
    /// Role assigned
    RoleAssigned {
        user_id: String,
        role: String,
    },
    /// Permission checked
    PermissionChecked {
        user_id: String,
        permission: String,
        resource: String,
        granted: bool,
    },
    /// Data accessed
    DataAccessed {
        user_id: String,
        resource: String,
    },
    /// Data modified
    DataModified {
        user_id: String,
        resource: String,
        operation: String,
    },
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: u64,
    pub event: AuditEvent,
}

/// Audit logger
pub struct AuditLogger {
    logs: Arc<RwLock<VecDeque<AuditLogEntry>>>,
    max_entries: usize,
}

impl AuditLogger {
    /// Create new audit logger
    pub fn new() -> Self {
        Self {
            logs: Arc::new(RwLock::new(VecDeque::new())),
            max_entries: 10000,
        }
    }

    /// Create with custom max entries
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            logs: Arc::new(RwLock::new(VecDeque::new())),
            max_entries,
        }
    }

    /// Log event
    pub fn log(&self, event: AuditEvent) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = AuditLogEntry { timestamp, event };

        let mut logs = self.logs.write();
        logs.push_back(entry);

        // Trim if exceeds max
        while logs.len() > self.max_entries {
            logs.pop_front();
        }
    }

    /// Get recent logs
    pub fn get_recent(&self, count: usize) -> Vec<AuditLogEntry> {
        let logs = self.logs.read();
        logs.iter()
            .rev()
            .take(count)
            .cloned()
            .collect()
    }

    /// Get logs for user
    pub fn get_user_logs(&self, user_id: &str) -> Vec<AuditLogEntry> {
        let logs = self.logs.read();
        logs.iter()
            .filter(|entry| match &entry.event {
                AuditEvent::UserCreated { user_id: uid, .. } => uid == user_id,
                AuditEvent::RoleAssigned { user_id: uid, .. } => uid == user_id,
                AuditEvent::PermissionChecked { user_id: uid, .. } => uid == user_id,
                AuditEvent::DataAccessed { user_id: uid, .. } => uid == user_id,
                AuditEvent::DataModified { user_id: uid, .. } => uid == user_id,
                _ => false,
            })
            .cloned()
            .collect()
    }

    /// Get log count
    pub fn log_count(&self) -> usize {
        let logs = self.logs.read();
        logs.len()
    }

    /// Clear logs
    pub fn clear(&mut self) {
        let mut logs = self.logs.write();
        logs.clear();
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_logger() {
        let logger = AuditLogger::new();
        
        logger.log(AuditEvent::UserCreated {
            user_id: "user1".to_string(),
            username: "alice".to_string(),
            tenant: "tenant1".to_string(),
        });
        
        assert_eq!(logger.log_count(), 1);
    }

    #[test]
    fn test_get_recent() {
        let logger = AuditLogger::new();
        
        for i in 0..5 {
            logger.log(AuditEvent::UserAuthenticated {
                username: format!("user{}", i),
            });
        }
        
        let recent = logger.get_recent(3);
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_max_entries() {
        let logger = AuditLogger::with_max_entries(10);
        
        for i in 0..20 {
            logger.log(AuditEvent::UserAuthenticated {
                username: format!("user{}", i),
            });
        }
        
        assert_eq!(logger.log_count(), 10);
    }

    #[test]
    fn test_get_user_logs() {
        let logger = AuditLogger::new();
        
        logger.log(AuditEvent::UserCreated {
            user_id: "user1".to_string(),
            username: "alice".to_string(),
            tenant: "tenant1".to_string(),
        });
        
        logger.log(AuditEvent::DataAccessed {
            user_id: "user1".to_string(),
            resource: "table1".to_string(),
        });
        
        logger.log(AuditEvent::UserCreated {
            user_id: "user2".to_string(),
            username: "bob".to_string(),
            tenant: "tenant1".to_string(),
        });
        
        let user1_logs = logger.get_user_logs("user1");
        assert_eq!(user1_logs.len(), 2);
    }
}
