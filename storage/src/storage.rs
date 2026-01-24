//! Main tiered storage implementation

use crate::error::{Result, TieredError};
use crate::migration::{MigrationDecision, MigrationManager};
use crate::policy::EvictionPolicy;
use crate::tier::{Tier, TierConfig, TierType};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

/// Data location
#[derive(Debug, Clone)]
struct DataLocation {
    tier: TierType,
    path: PathBuf,
    size: u64,
}

/// Tiered storage
pub struct TieredStorage {
    /// Tiers
    tiers: Arc<RwLock<HashMap<TierType, Tier>>>,
    /// Data locations
    locations: Arc<RwLock<HashMap<String, DataLocation>>>,
    /// Migration manager
    migration_manager: Arc<MigrationManager>,
}

impl TieredStorage {
    /// Create builder
    pub fn builder() -> TieredStorageBuilder {
        TieredStorageBuilder::new()
    }

    /// Put data
    pub async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        let size = data.len() as u64;

        // Find best tier with space
        let tier_type = self.find_tier_with_space(size)?;

        // Get tier path
        let path = {
            let tiers = self.tiers.read();
            let tier = tiers.get(&tier_type)
                .ok_or_else(|| TieredError::InvalidConfig("Tier not found".to_string()))?;
            tier.config().path.join(key)
        };

        // Write data
        fs::write(&path, data).await?;

        // Update location
        {
            let mut locations = self.locations.write();
            locations.insert(
                key.to_string(),
                DataLocation {
                    tier: tier_type,
                    path: path.clone(),
                    size,
                },
            );
        }

        // Update tier size
        {
            let mut tiers = self.tiers.write();
            if let Some(tier) = tiers.get_mut(&tier_type) {
                tier.update_size(size as i64);
            }
        }

        // Track in migration manager
        self.migration_manager.track(tier_type, key.to_string(), size);

        Ok(())
    }

    /// Get data
    pub async fn get(&self, key: &str) -> Result<Vec<u8>> {
        // Get location
        let location = {
            let locations = self.locations.read();
            locations
                .get(key)
                .cloned()
                .ok_or_else(|| TieredError::KeyNotFound(key.to_string()))?
        };

        // Read data
        let data = fs::read(&location.path).await?;

        // Record access
        self.migration_manager.record_access(location.tier, key);

        Ok(data)
    }

    /// Delete data
    pub async fn delete(&self, key: &str) -> Result<()> {
        // Get location
        let location = {
            let mut locations = self.locations.write();
            locations
                .remove(key)
                .ok_or_else(|| TieredError::KeyNotFound(key.to_string()))?
        };

        // Delete file
        fs::remove_file(&location.path).await?;

        // Update tier size
        {
            let mut tiers = self.tiers.write();
            if let Some(tier) = tiers.get_mut(&location.tier) {
                tier.update_size(-(location.size as i64));
            }
        }

        // Remove from migration manager
        self.migration_manager.remove(location.tier, key);

        Ok(())
    }

    /// Check if key exists
    pub fn exists(&self, key: &str) -> bool {
        let locations = self.locations.read();
        locations.contains_key(key)
    }

    /// Get key count
    pub fn key_count(&self) -> usize {
        let locations = self.locations.read();
        locations.len()
    }

    /// Find tier with space
    fn find_tier_with_space(&self, size: u64) -> Result<TierType> {
        let tiers = self.tiers.read();

        // Try hot tier first
        if let Some(tier) = tiers.get(&TierType::Hot) {
            if tier.has_space(size) {
                return Ok(TierType::Hot);
            }
        }

        // Try warm tier
        if let Some(tier) = tiers.get(&TierType::Warm) {
            if tier.has_space(size) {
                return Ok(TierType::Warm);
            }
        }

        // Try cold tier
        if let Some(tier) = tiers.get(&TierType::Cold) {
            if tier.has_space(size) {
                return Ok(TierType::Cold);
            }
        }

        Err(TieredError::TierFull("All tiers full".to_string()))
    }

    /// Run migration analysis
    pub fn analyze_migrations(&self) -> Vec<MigrationDecision> {
        self.migration_manager.analyze_migrations()
    }

    /// Execute migration
    pub async fn migrate(&self, decision: &MigrationDecision) -> Result<()> {
        // Get current location
        let location = {
            let locations = self.locations.read();
            locations
                .get(&decision.key)
                .cloned()
                .ok_or_else(|| TieredError::KeyNotFound(decision.key.clone()))?
        };

        // Read data
        let data = fs::read(&location.path).await?;

        // Get new path
        let new_path = {
            let tiers = self.tiers.read();
            let tier = tiers.get(&decision.to_tier)
                .ok_or_else(|| TieredError::InvalidConfig("Target tier not found".to_string()))?;
            tier.config().path.join(&decision.key)
        };

        // Write to new tier
        fs::write(&new_path, &data).await?;

        // Delete from old tier
        fs::remove_file(&location.path).await?;

        // Update location
        {
            let mut locations = self.locations.write();
            locations.insert(
                decision.key.clone(),
                DataLocation {
                    tier: decision.to_tier,
                    path: new_path,
                    size: location.size,
                },
            );
        }

        // Update tier sizes
        {
            let mut tiers = self.tiers.write();
            
            // Decrease old tier
            if let Some(tier) = tiers.get_mut(&decision.from_tier) {
                tier.update_size(-(location.size as i64));
            }
            
            // Increase new tier
            if let Some(tier) = tiers.get_mut(&decision.to_tier) {
                tier.update_size(location.size as i64);
            }
        }

        // Update migration manager
        self.migration_manager.remove(decision.from_tier, &decision.key);
        self.migration_manager.track(decision.to_tier, decision.key.clone(), location.size);
        self.migration_manager.record_migration(decision, location.size);

        Ok(())
    }

    /// Get tier statistics
    pub fn tier_stats(&self) -> HashMap<TierType, TierStats> {
        let tiers = self.tiers.read();
        tiers
            .iter()
            .map(|(tier_type, tier)| {
                let config = tier.config();
                (
                    *tier_type,
                    TierStats {
                        current_size: config.current_size,
                        max_size: config.max_size,
                        usage_percentage: config.usage_percentage(),
                        key_count: self.migration_manager.pattern_counts()
                            .get(tier_type)
                            .copied()
                            .unwrap_or(0),
                    },
                )
            })
            .collect()
    }
}

/// Tier statistics
#[derive(Debug, Clone)]
pub struct TierStats {
    pub current_size: u64,
    pub max_size: u64,
    pub usage_percentage: f64,
    pub key_count: usize,
}

/// Tiered storage builder
pub struct TieredStorageBuilder {
    hot_config: Option<TierConfig>,
    warm_config: Option<TierConfig>,
    cold_config: Option<TierConfig>,
    eviction_policy: EvictionPolicy,
}

impl TieredStorageBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            hot_config: None,
            warm_config: None,
            cold_config: None,
            eviction_policy: EvictionPolicy::LRU,
        }
    }

    /// Configure hot tier
    pub fn hot_tier<P: Into<PathBuf>>(mut self, path: P, max_size: u64) -> Self {
        self.hot_config = Some(TierConfig::new(TierType::Hot, path.into(), max_size));
        self
    }

    /// Configure warm tier
    pub fn warm_tier<P: Into<PathBuf>>(mut self, path: P, max_size: u64) -> Self {
        self.warm_config = Some(TierConfig::new(TierType::Warm, path.into(), max_size));
        self
    }

    /// Configure cold tier
    pub fn cold_tier<P: Into<PathBuf>>(mut self, path: P, max_size: u64) -> Self {
        self.cold_config = Some(TierConfig::new(TierType::Cold, path.into(), max_size));
        self
    }

    /// Set eviction policy
    pub fn eviction_policy(mut self, policy: EvictionPolicy) -> Self {
        self.eviction_policy = policy;
        self
    }

    /// Build tiered storage
    pub async fn build(self) -> Result<TieredStorage> {
        let mut tiers = HashMap::new();

        // Create tiers
        if let Some(config) = self.hot_config {
            fs::create_dir_all(&config.path).await?;
            tiers.insert(TierType::Hot, Tier::new(config));
        }

        if let Some(config) = self.warm_config {
            fs::create_dir_all(&config.path).await?;
            tiers.insert(TierType::Warm, Tier::new(config));
        }

        if let Some(config) = self.cold_config {
            fs::create_dir_all(&config.path).await?;
            tiers.insert(TierType::Cold, Tier::new(config));
        }

        if tiers.is_empty() {
            return Err(TieredError::InvalidConfig(
                "At least one tier must be configured".to_string(),
            ));
        }

        Ok(TieredStorage {
            tiers: Arc::new(RwLock::new(tiers)),
            locations: Arc::new(RwLock::new(HashMap::new())),
            migration_manager: Arc::new(MigrationManager::new(self.eviction_policy)),
        })
    }
}

impl Default for TieredStorageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_tiered_storage_basic() {
        let temp_dir = TempDir::new().unwrap();
        let hot_path = temp_dir.path().join("hot");
        let warm_path = temp_dir.path().join("warm");

        let storage = TieredStorage::builder()
            .hot_tier(hot_path, 1024 * 1024)
            .warm_tier(warm_path, 10 * 1024 * 1024)
            .build()
            .await
            .unwrap();

        // Put data
        storage.put("key1", b"data1").await.unwrap();
        assert!(storage.exists("key1"));

        // Get data
        let data = storage.get("key1").await.unwrap();
        assert_eq!(data, b"data1");

        // Delete data
        storage.delete("key1").await.unwrap();
        assert!(!storage.exists("key1"));
    }

    #[tokio::test]
    async fn test_tier_overflow() {
        let temp_dir = TempDir::new().unwrap();
        let hot_path = temp_dir.path().join("hot");
        let warm_path = temp_dir.path().join("warm");

        let storage = TieredStorage::builder()
            .hot_tier(hot_path, 100) // Small hot tier
            .warm_tier(warm_path, 1024 * 1024)
            .build()
            .await
            .unwrap();

        // Put small data (goes to hot)
        storage.put("key1", b"small").await.unwrap();

        // Put large data (should go to warm)
        storage.put("key2", &vec![0u8; 200]).await.unwrap();

        assert!(storage.exists("key1"));
        assert!(storage.exists("key2"));
    }

    #[tokio::test]
    async fn test_tier_stats() {
        let temp_dir = TempDir::new().unwrap();
        let hot_path = temp_dir.path().join("hot");

        let storage = TieredStorage::builder()
            .hot_tier(hot_path, 1024 * 1024)
            .build()
            .await
            .unwrap();

        storage.put("key1", b"data1").await.unwrap();
        storage.put("key2", b"data2").await.unwrap();

        let stats = storage.tier_stats();
        let hot_stats = stats.get(&TierType::Hot).unwrap();
        
        assert_eq!(hot_stats.key_count, 2);
        assert!(hot_stats.current_size > 0);
    }
}
