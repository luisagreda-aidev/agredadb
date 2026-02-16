//! Configuration for AgredaDB server

use serde::{Deserialize, Serialize};
use crate::durability::DurabilityMode;

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Durability mode
    pub durability_mode: DurabilityMode,

    /// Adaptive timeout in microseconds for group commit
    pub adaptive_timeout_micros: u64,

    /// Minimum batch size to process immediately
    pub min_batch_for_immediate_process: usize,

    /// Maximum drain size for batch processing
    pub max_drain_size: usize,

    /// WAL sync policy parameters
    pub wal_sync_batch_size: usize,
    pub wal_sync_max_delay_micros: u64,

    /// Enable metrics collection
    pub enable_metrics: bool,

    /// Metrics reporting interval in seconds
    pub metrics_report_interval_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            durability_mode: DurabilityMode::Async,
            adaptive_timeout_micros: 50,
            min_batch_for_immediate_process: 10,
            max_drain_size: 10_000,
            wal_sync_batch_size: 50,
            wal_sync_max_delay_micros: 1000,
            enable_metrics: true,
            metrics_report_interval_secs: 10,
        }
    }
}

impl ServerConfig {
    /// Create a configuration optimized for low latency (1-to-1 mode)
    pub fn low_latency() -> Self {
        Self {
            durability_mode: DurabilityMode::Async,
            adaptive_timeout_micros: 20,
            min_batch_for_immediate_process: 5,
            max_drain_size: 1_000,
            wal_sync_batch_size: 10,
            wal_sync_max_delay_micros: 500,
            enable_metrics: true,
            metrics_report_interval_secs: 5,
        }
    }

    /// Create a configuration optimized for high throughput (batch mode)
    pub fn high_throughput() -> Self {
        Self {
            durability_mode: DurabilityMode::Async,
            adaptive_timeout_micros: 100,
            min_batch_for_immediate_process: 50,
            max_drain_size: 50_000,
            wal_sync_batch_size: 1000,
            wal_sync_max_delay_micros: 5000,
            enable_metrics: true,
            metrics_report_interval_secs: 10,
        }
    }

    /// Create a configuration for maximum safety (no data loss)
    pub fn maximum_safety() -> Self {
        Self {
            durability_mode: DurabilityMode::Strict,
            adaptive_timeout_micros: 10,
            min_batch_for_immediate_process: 1,
            max_drain_size: 100,
            wal_sync_batch_size: 1,
            wal_sync_max_delay_micros: 100,
            enable_metrics: true,
            metrics_report_interval_secs: 10,
        }
    }
}
