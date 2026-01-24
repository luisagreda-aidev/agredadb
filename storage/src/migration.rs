//! Automatic data migration between tiers

use crate::policy::{AccessPattern, EvictionPolicy};
use crate::tier::TierType;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Migration decision
#[derive(Debug, Clone)]
pub struct MigrationDecision {
    /// Key to migrate
    pub key: String,
    /// Source tier
    pub from_tier: TierType,
    /// Destination tier
    pub to_tier: TierType,
    /// Reason for migration
    pub reason: MigrationReason,
}

/// Migration reason
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationReason {
    /// Frequently accessed (promote to hot)
    HighAccess,
    /// Rarely accessed (demote to cold)
    LowAccess,
    /// Tier full (need to evict)
    TierFull,
    /// TTL expired
    Expired,
    /// Manual migration
    Manual,
}

/// Migration statistics
#[derive(Debug, Clone, Default)]
pub struct MigrationStats {
    /// Total migrations
    pub total_migrations: u64,
    /// Promotions (to hotter tier)
    pub promotions: u64,
    /// Demotions (to colder tier)
    pub demotions: u64,
    /// Bytes migrated
    pub bytes_migrated: u64,
}

/// Migration manager
pub struct MigrationManager {
    /// Access patterns per tier
    patterns: Arc<RwLock<HashMap<TierType, HashMap<String, AccessPattern>>>>,
    /// Migration statistics
    stats: Arc<RwLock<MigrationStats>>,
    /// Eviction policy
    policy: EvictionPolicy,
    /// Hot tier access threshold (accesses per hour)
    hot_threshold: u64,
    /// Cold tier idle threshold (hours)
    cold_threshold: u64,
}

impl MigrationManager {
    /// Create new migration manager
    pub fn new(policy: EvictionPolicy) -> Self {
        Self {
            patterns: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(MigrationStats::default())),
            policy,
            hot_threshold: 100,  // 100 accesses/hour for hot tier
            cold_threshold: 24,  // 24 hours idle for cold tier
        }
    }

    /// Track key in tier
    pub fn track(&self, tier: TierType, key: String, size: u64) {
        let mut patterns = self.patterns.write();
        let tier_patterns = patterns.entry(tier).or_insert_with(HashMap::new);
        tier_patterns.insert(key.clone(), AccessPattern::new(key, size, None));
    }

    /// Record access
    pub fn record_access(&self, tier: TierType, key: &str) {
        let mut patterns = self.patterns.write();
        if let Some(tier_patterns) = patterns.get_mut(&tier) {
            if let Some(pattern) = tier_patterns.get_mut(key) {
                pattern.record_access();
            }
        }
    }

    /// Remove key
    pub fn remove(&self, tier: TierType, key: &str) {
        let mut patterns = self.patterns.write();
        if let Some(tier_patterns) = patterns.get_mut(&tier) {
            tier_patterns.remove(key);
        }
    }

    /// Analyze and suggest migrations
    pub fn analyze_migrations(&self) -> Vec<MigrationDecision> {
        let patterns = self.patterns.read();
        let mut decisions = Vec::new();

        // Check warm tier for promotion to hot
        if let Some(warm_patterns) = patterns.get(&TierType::Warm) {
            for pattern in warm_patterns.values() {
                if self.should_promote_to_hot(pattern) {
                    decisions.push(MigrationDecision {
                        key: pattern.key.clone(),
                        from_tier: TierType::Warm,
                        to_tier: TierType::Hot,
                        reason: MigrationReason::HighAccess,
                    });
                }
            }
        }

        // Check hot tier for demotion to warm
        if let Some(hot_patterns) = patterns.get(&TierType::Hot) {
            for pattern in hot_patterns.values() {
                if self.should_demote_to_warm(pattern) {
                    decisions.push(MigrationDecision {
                        key: pattern.key.clone(),
                        from_tier: TierType::Hot,
                        to_tier: TierType::Warm,
                        reason: MigrationReason::LowAccess,
                    });
                }
            }
        }

        // Check warm tier for demotion to cold
        if let Some(warm_patterns) = patterns.get(&TierType::Warm) {
            for pattern in warm_patterns.values() {
                if self.should_demote_to_cold(pattern) {
                    decisions.push(MigrationDecision {
                        key: pattern.key.clone(),
                        from_tier: TierType::Warm,
                        to_tier: TierType::Cold,
                        reason: MigrationReason::LowAccess,
                    });
                }
            }
        }

        decisions
    }

    /// Check if should promote to hot tier
    fn should_promote_to_hot(&self, pattern: &AccessPattern) -> bool {
        // Calculate accesses per hour
        let age_hours = pattern.age_seconds() as f64 / 3600.0;
        if age_hours < 0.1 {
            return false; // Too new to decide
        }

        let accesses_per_hour = pattern.access_count as f64 / age_hours;
        accesses_per_hour >= self.hot_threshold as f64
    }

    /// Check if should demote to warm tier
    fn should_demote_to_warm(&self, pattern: &AccessPattern) -> bool {
        // Check if idle for too long
        let idle_hours = pattern.idle_seconds() as f64 / 3600.0;
        idle_hours >= (self.cold_threshold / 2) as f64
    }

    /// Check if should demote to cold tier
    fn should_demote_to_cold(&self, pattern: &AccessPattern) -> bool {
        // Check if idle for too long
        let idle_hours = pattern.idle_seconds() as f64 / 3600.0;
        idle_hours >= self.cold_threshold as f64
    }

    /// Select keys to evict from tier
    pub fn select_eviction_candidates(
        &self,
        tier: TierType,
        count: usize,
    ) -> Vec<String> {
        let patterns = self.patterns.read();
        
        if let Some(tier_patterns) = patterns.get(&tier) {
            let mut patterns_vec: Vec<_> = tier_patterns.values().collect();

            match self.policy {
                EvictionPolicy::LRU => {
                    patterns_vec.sort_by_key(|p| p.last_access);
                }
                EvictionPolicy::LFU => {
                    patterns_vec.sort_by_key(|p| p.access_count);
                }
                EvictionPolicy::TTL => {
                    patterns_vec.sort_by_key(|p| p.created_at);
                }
                EvictionPolicy::Size => {
                    patterns_vec.sort_by_key(|p| std::cmp::Reverse(p.size));
                }
            }

            return patterns_vec
                .into_iter()
                .take(count)
                .map(|p| p.key.clone())
                .collect();
        }

        Vec::new()
    }

    /// Record migration
    pub fn record_migration(&self, decision: &MigrationDecision, size: u64) {
        let mut stats = self.stats.write();
        stats.total_migrations += 1;
        stats.bytes_migrated += size;

        if decision.to_tier.priority() > decision.from_tier.priority() {
            stats.promotions += 1;
        } else {
            stats.demotions += 1;
        }
    }

    /// Get statistics
    pub fn stats(&self) -> MigrationStats {
        self.stats.read().clone()
    }

    /// Get pattern count per tier
    pub fn pattern_counts(&self) -> HashMap<TierType, usize> {
        let patterns = self.patterns.read();
        patterns
            .iter()
            .map(|(tier, patterns)| (*tier, patterns.len()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_migration_manager() {
        let manager = MigrationManager::new(EvictionPolicy::LRU);
        
        manager.track(TierType::Hot, "key1".to_string(), 1024);
        manager.track(TierType::Warm, "key2".to_string(), 2048);
        
        let counts = manager.pattern_counts();
        assert_eq!(counts.get(&TierType::Hot), Some(&1));
        assert_eq!(counts.get(&TierType::Warm), Some(&1));
    }

    #[test]
    fn test_eviction_candidates() {
        let manager = MigrationManager::new(EvictionPolicy::LRU);
        
        manager.track(TierType::Hot, "key1".to_string(), 1024);
        manager.track(TierType::Hot, "key2".to_string(), 2048);
        manager.track(TierType::Hot, "key3".to_string(), 512);

        sleep(Duration::from_millis(10));
        manager.record_access(TierType::Hot, "key2");

        let candidates = manager.select_eviction_candidates(TierType::Hot, 1);
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn test_migration_stats() {
        let manager = MigrationManager::new(EvictionPolicy::LRU);
        
        let decision = MigrationDecision {
            key: "key1".to_string(),
            from_tier: TierType::Warm,
            to_tier: TierType::Hot,
            reason: MigrationReason::HighAccess,
        };

        manager.record_migration(&decision, 1024);

        let stats = manager.stats();
        assert_eq!(stats.total_migrations, 1);
        assert_eq!(stats.promotions, 1);
        assert_eq!(stats.bytes_migrated, 1024);
    }
}
