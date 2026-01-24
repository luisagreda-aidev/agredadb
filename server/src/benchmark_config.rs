//! Benchmark configuration module
//! Allows switching between different durability modes for testing

use agredadb_wal::SyncPolicy;

/// Benchmark mode configuration
#[derive(Debug, Clone, Copy)]
pub enum BenchmarkMode {
    /// Maximum performance (no fsync) - for comparison with baseline
    MaxPerformance,
    /// Real durability (with fsync) - for production comparison
    RealDurability,
    /// Adaptive (balanced) - for general use
    Adaptive,
}

impl BenchmarkMode {
    /// Get the WAL sync policy for this benchmark mode
    pub fn sync_policy(&self) -> SyncPolicy {
        match self {
            BenchmarkMode::MaxPerformance => SyncPolicy::Never,
            BenchmarkMode::RealDurability => SyncPolicy::EveryWrite,
            BenchmarkMode::Adaptive => SyncPolicy::Adaptive(50, 1000),
        }
    }
    
    /// Get a description of this mode
    pub fn description(&self) -> &'static str {
        match self {
            BenchmarkMode::MaxPerformance => "Máximo Rendimiento (sin fsync) - Comparación con baseline",
            BenchmarkMode::RealDurability => "Durabilidad Real (con fsync) - Comparación con producción",
            BenchmarkMode::Adaptive => "Adaptativo (balanceado) - Uso general",
        }
    }
}

/// Get the current benchmark mode from environment variable
/// Default is Adaptive if not set
pub fn get_benchmark_mode() -> BenchmarkMode {
    match std::env::var("AGREDA_BENCHMARK_MODE").as_deref() {
        Ok("max_performance") | Ok("MAX_PERFORMANCE") => BenchmarkMode::MaxPerformance,
        Ok("real_durability") | Ok("REAL_DURABILITY") => BenchmarkMode::RealDurability,
        _ => BenchmarkMode::Adaptive,
    }
}
