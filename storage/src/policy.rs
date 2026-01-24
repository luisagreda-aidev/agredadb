//! Eviction policies and access pattern tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Eviction policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvictionPolicy {
    /// Least Recently Used
    LRU,
    /// Least Frequently Used
    LFU,
    /// Time To Live
    TTL,
    /// Size-based (evict largest first)
    Size,
}

/// Access pattern for a key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPattern {
    /// Key
    pub key: String,
    /// Last access time
    pub last_access: SystemTime,
    /// Access count
    pub access_count: u64,
    /// Data size
    pub size: u64,
    /// Creation time
    pub created_at: SystemTime,
    /// Time to live (optional)
    pub ttl: Option<Duration>,
}

impl AccessPattern {
    /// Create new access pattern
    pub fn new(key: String, size: u64, ttl: Option<Duration>) -> Self {
        let now = SystemTime::now();
        Self {
            key,
            last_access: now,
            access_count: 0,
            size,
            created_at: now,
            ttl,
        }
    }

    /// Record access
    pub fn record_access(&mut self) {
        self.last_access = SystemTime::now();
        self.access_count += 1;
    }

    /// Check if expired (for TTL policy)
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            if let Ok(elapsed) = self.created_at.elapsed() {
                return elapsed > ttl;
            }
        }
        false
    }

    /// Get age in seconds
    pub fn age_seconds(&self) -> u64 {
        self.created_at
            .elapsed()
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Get time since last access in seconds
    pub fn idle_seconds(&self) -> u64 {
        self.last_access
            .elapsed()
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// Policy manager
pub struct PolicyManager {
    policy: EvictionPolicy,
    patterns: HashMap<String, AccessPattern>,
}

impl PolicyManager {
    /// Create new policy manager
    pub fn new(policy: EvictionPolicy) -> Self {
        Self {
            policy,
            patterns: HashMap::new(),
        }
    }

    /// Track new key
    pub fn track(&mut self, key: String, size: u64, ttl: Option<Duration>) {
        self.patterns.insert(key.clone(), AccessPattern::new(key, size, ttl));
    }

    /// Record access
    pub fn record_access(&mut self, key: &str) {
        if let Some(pattern) = self.patterns.get_mut(key) {
            pattern.record_access();
        }
    }

    /// Remove key
    pub fn remove(&mut self, key: &str) {
        self.patterns.remove(key);
    }

    /// Select keys to evict
    pub fn select_eviction_candidates(&self, count: usize) -> Vec<String> {
        let mut patterns: Vec<_> = self.patterns.values().collect();

        match self.policy {
            EvictionPolicy::LRU => {
                // Sort by last access time (oldest first)
                patterns.sort_by_key(|p| p.last_access);
            }
            EvictionPolicy::LFU => {
                // Sort by access count (least accessed first)
                patterns.sort_by_key(|p| p.access_count);
            }
            EvictionPolicy::TTL => {
                // Sort by age (oldest first)
                patterns.sort_by_key(|p| p.created_at);
            }
            EvictionPolicy::Size => {
                // Sort by size (largest first)
                patterns.sort_by_key(|p| std::cmp::Reverse(p.size));
            }
        }

        patterns
            .into_iter()
            .take(count)
            .map(|p| p.key.clone())
            .collect()
    }

    /// Get expired keys (for TTL policy)
    pub fn get_expired_keys(&self) -> Vec<String> {
        self.patterns
            .values()
            .filter(|p| p.is_expired())
            .map(|p| p.key.clone())
            .collect()
    }

    /// Get statistics
    pub fn stats(&self) -> PolicyStats {
        let total_keys = self.patterns.len();
        let total_size: u64 = self.patterns.values().map(|p| p.size).sum();
        let total_accesses: u64 = self.patterns.values().map(|p| p.access_count).sum();

        PolicyStats {
            total_keys,
            total_size,
            total_accesses,
            avg_access_count: if total_keys > 0 {
                total_accesses / total_keys as u64
            } else {
                0
            },
        }
    }
}

/// Policy statistics
#[derive(Debug, Clone)]
pub struct PolicyStats {
    pub total_keys: usize,
    pub total_size: u64,
    pub total_accesses: u64,
    pub avg_access_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_access_pattern() {
        let mut pattern = AccessPattern::new("key1".to_string(), 1024, None);
        
        assert_eq!(pattern.access_count, 0);
        
        pattern.record_access();
        assert_eq!(pattern.access_count, 1);
        
        pattern.record_access();
        assert_eq!(pattern.access_count, 2);
    }

    #[test]
    fn test_ttl_expiration() {
        let pattern = AccessPattern::new(
            "key1".to_string(),
            1024,
            Some(Duration::from_millis(100)),
        );

        assert!(!pattern.is_expired());
        
        sleep(Duration::from_millis(150));
        assert!(pattern.is_expired());
    }

    #[test]
    fn test_policy_manager_lru() {
        let mut manager = PolicyManager::new(EvictionPolicy::LRU);
        
        manager.track("key1".to_string(), 1024, None);
        manager.track("key2".to_string(), 2048, None);
        manager.track("key3".to_string(), 512, None);

        // Access key2 to make it more recent
        sleep(Duration::from_millis(10));
        manager.record_access("key2");

        // key1 should be selected for eviction (oldest)
        let candidates = manager.select_eviction_candidates(1);
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn test_policy_manager_lfu() {
        let mut manager = PolicyManager::new(EvictionPolicy::LFU);
        
        manager.track("key1".to_string(), 1024, None);
        manager.track("key2".to_string(), 2048, None);
        manager.track("key3".to_string(), 512, None);

        // Access key2 multiple times
        manager.record_access("key2");
        manager.record_access("key2");
        manager.record_access("key2");

        // key1 or key3 should be selected (least accessed)
        let candidates = manager.select_eviction_candidates(1);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0] == "key1" || candidates[0] == "key3");
    }

    #[test]
    fn test_policy_stats() {
        let mut manager = PolicyManager::new(EvictionPolicy::LRU);
        
        manager.track("key1".to_string(), 1024, None);
        manager.track("key2".to_string(), 2048, None);
        
        manager.record_access("key1");
        manager.record_access("key1");
        manager.record_access("key2");

        let stats = manager.stats();
        assert_eq!(stats.total_keys, 2);
        assert_eq!(stats.total_size, 3072);
        assert_eq!(stats.total_accesses, 3);
    }
}
