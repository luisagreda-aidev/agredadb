//! Tier types and configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Tier type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TierType {
    /// Hot tier - NVMe storage (<1ms latency)
    Hot,
    /// Warm tier - SSD storage (<10ms latency)
    Warm,
    /// Cold tier - HDD/S3 storage (<100ms latency)
    Cold,
}

impl TierType {
    /// Get expected latency for this tier
    pub fn expected_latency(&self) -> Duration {
        match self {
            Self::Hot => Duration::from_micros(500),   // <1ms
            Self::Warm => Duration::from_millis(5),    // <10ms
            Self::Cold => Duration::from_millis(50),   // <100ms
        }
    }

    /// Get priority (higher = more preferred)
    pub fn priority(&self) -> u8 {
        match self {
            Self::Hot => 3,
            Self::Warm => 2,
            Self::Cold => 1,
        }
    }
}

/// Tier configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Tier type
    pub tier_type: TierType,
    /// Storage path
    pub path: PathBuf,
    /// Maximum size in bytes
    pub max_size: u64,
    /// Current size in bytes
    pub current_size: u64,
}

impl TierConfig {
    /// Create new tier configuration
    pub fn new(tier_type: TierType, path: PathBuf, max_size: u64) -> Self {
        Self {
            tier_type,
            path,
            max_size,
            current_size: 0,
        }
    }

    /// Check if tier has space
    pub fn has_space(&self, size: u64) -> bool {
        self.current_size + size <= self.max_size
    }

    /// Get available space
    pub fn available_space(&self) -> u64 {
        self.max_size.saturating_sub(self.current_size)
    }

    /// Get usage percentage
    pub fn usage_percentage(&self) -> f64 {
        (self.current_size as f64 / self.max_size as f64) * 100.0
    }
}

/// Tier implementation
pub struct Tier {
    config: TierConfig,
}

impl Tier {
    /// Create new tier
    pub fn new(config: TierConfig) -> Self {
        Self { config }
    }

    /// Get tier type
    pub fn tier_type(&self) -> TierType {
        self.config.tier_type
    }

    /// Get configuration
    pub fn config(&self) -> &TierConfig {
        &self.config
    }

    /// Check if has space
    pub fn has_space(&self, size: u64) -> bool {
        self.config.has_space(size)
    }

    /// Update size
    pub fn update_size(&mut self, delta: i64) {
        if delta >= 0 {
            self.config.current_size += delta as u64;
        } else {
            self.config.current_size = self.config.current_size.saturating_sub((-delta) as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_type_latency() {
        assert!(TierType::Hot.expected_latency() < TierType::Warm.expected_latency());
        assert!(TierType::Warm.expected_latency() < TierType::Cold.expected_latency());
    }

    #[test]
    fn test_tier_type_priority() {
        assert!(TierType::Hot.priority() > TierType::Warm.priority());
        assert!(TierType::Warm.priority() > TierType::Cold.priority());
    }

    #[test]
    fn test_tier_config() {
        let config = TierConfig::new(
            TierType::Hot,
            PathBuf::from("/nvme/hot"),
            1024 * 1024 * 1024, // 1GB
        );

        assert!(config.has_space(512 * 1024 * 1024)); // 512MB
        assert_eq!(config.usage_percentage(), 0.0);
    }

    #[test]
    fn test_tier_space_management() {
        let mut tier = Tier::new(TierConfig::new(
            TierType::Hot,
            PathBuf::from("/nvme/hot"),
            1024,
        ));

        assert!(tier.has_space(512));
        
        tier.update_size(512);
        assert_eq!(tier.config().current_size, 512);
        
        tier.update_size(-256);
        assert_eq!(tier.config().current_size, 256);
    }
}
