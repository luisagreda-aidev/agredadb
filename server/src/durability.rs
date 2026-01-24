//! Durability modes for AgredaDB
//! Allows choosing between performance and durability guarantees

use serde::{Deserialize, Serialize};

/// Durability mode determines how writes are persisted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurabilityMode {
    /// MEMORY: Maximum performance (3.3M RPS)
    /// - No WAL, no fsync
    /// - Data only in memory
    /// - Lost on crash
    /// - Use for: caches, sessions, temporary data
    /// - Equivalent to: Redis, Memcached
    Memory,
    
    /// ASYNC: High performance with eventual durability (2M RPS)
    /// - WAL with background fsync (every 1-10ms)
    /// - Maximum data loss: 1-10ms of writes
    /// - Use for: logs, analytics, non-critical data
    /// - Equivalent to: PostgreSQL (default), MongoDB
    Async,
    
    /// STRICT: Maximum durability (112K RPS)
    /// - WAL with synchronous fsync
    /// - No data loss
    /// - Use for: financial transactions, critical data
    /// - Equivalent to: PostgreSQL (synchronous_commit=on)
    Strict,
}

impl DurabilityMode {
    /// Get human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            DurabilityMode::Memory => "MEMORY: 3.3M RPS, no durability (cache mode)",
            DurabilityMode::Async => "ASYNC: 2M RPS, eventual durability (1-10ms loss)",
            DurabilityMode::Strict => "STRICT: 112K RPS, full durability (no loss)",
        }
    }
    
    /// Get expected throughput
    pub fn expected_throughput(&self) -> &'static str {
        match self {
            DurabilityMode::Memory => "3.3M RPS",
            DurabilityMode::Async => "2M RPS",
            DurabilityMode::Strict => "112K RPS",
        }
    }
    
    /// Get durability guarantee
    pub fn durability_guarantee(&self) -> &'static str {
        match self {
            DurabilityMode::Memory => "None (data lost on crash)",
            DurabilityMode::Async => "Eventual (max 1-10ms loss)",
            DurabilityMode::Strict => "Full (no data loss)",
        }
    }
    
    /// Check if this mode uses WAL
    pub fn uses_wal(&self) -> bool {
        match self {
            DurabilityMode::Memory => false,
            DurabilityMode::Async => true,
            DurabilityMode::Strict => true,
        }
    }
    
    /// Check if this mode does synchronous fsync
    pub fn sync_fsync(&self) -> bool {
        match self {
            DurabilityMode::Memory => false,
            DurabilityMode::Async => false,
            DurabilityMode::Strict => true,
        }
    }
}

impl Default for DurabilityMode {
    fn default() -> Self {
        // Default to ASYNC for good balance
        DurabilityMode::Async
    }
}

impl std::fmt::Display for DurabilityMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DurabilityMode::Memory => write!(f, "MEMORY"),
            DurabilityMode::Async => write!(f, "ASYNC"),
            DurabilityMode::Strict => write!(f, "STRICT"),
        }
    }
}

impl std::str::FromStr for DurabilityMode {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "MEMORY" => Ok(DurabilityMode::Memory),
            "ASYNC" => Ok(DurabilityMode::Async),
            "STRICT" => Ok(DurabilityMode::Strict),
            _ => Err(format!("Invalid durability mode: {}. Valid options: MEMORY, ASYNC, STRICT", s)),
        }
    }
}

/// Get durability mode from environment variable
pub fn get_durability_mode() -> DurabilityMode {
    std::env::var("AGREDA_DURABILITY_MODE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_durability_modes() {
        assert_eq!(DurabilityMode::Memory.uses_wal(), false);
        assert_eq!(DurabilityMode::Async.uses_wal(), true);
        assert_eq!(DurabilityMode::Strict.uses_wal(), true);
        
        assert_eq!(DurabilityMode::Memory.sync_fsync(), false);
        assert_eq!(DurabilityMode::Async.sync_fsync(), false);
        assert_eq!(DurabilityMode::Strict.sync_fsync(), true);
    }
    
    #[test]
    fn test_parse() {
        assert_eq!("MEMORY".parse::<DurabilityMode>().unwrap(), DurabilityMode::Memory);
        assert_eq!("async".parse::<DurabilityMode>().unwrap(), DurabilityMode::Async);
        assert_eq!("Strict".parse::<DurabilityMode>().unwrap(), DurabilityMode::Strict);
    }
}
