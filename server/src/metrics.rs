//! Performance metrics and monitoring for AgredaDB

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Performance metrics collector
#[derive(Clone)]
pub struct Metrics {
    // Throughput metrics
    total_inserts: Arc<AtomicUsize>,
    total_searches: Arc<AtomicUsize>,
    total_batches: Arc<AtomicUsize>,
    
    // Latency metrics (in microseconds)
    insert_latency_sum: Arc<AtomicU64>,
    insert_latency_count: Arc<AtomicU64>,
    
    // Batch metrics
    batch_size_sum: Arc<AtomicUsize>,
    batch_count: Arc<AtomicUsize>,
    
    // Timing
    start_time: Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            total_inserts: Arc::new(AtomicUsize::new(0)),
            total_searches: Arc::new(AtomicUsize::new(0)),
            total_batches: Arc::new(AtomicUsize::new(0)),
            insert_latency_sum: Arc::new(AtomicU64::new(0)),
            insert_latency_count: Arc::new(AtomicU64::new(0)),
            batch_size_sum: Arc::new(AtomicUsize::new(0)),
            batch_count: Arc::new(AtomicUsize::new(0)),
            start_time: Instant::now(),
        }
    }
    
    /// Record an insert operation
    pub fn record_insert(&self, count: usize, latency: Duration) {
        self.total_inserts.fetch_add(count, Ordering::Relaxed);
        self.insert_latency_sum.fetch_add(latency.as_micros() as u64, Ordering::Relaxed);
        self.insert_latency_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a batch operation
    pub fn record_batch(&self, size: usize) {
        self.total_batches.fetch_add(1, Ordering::Relaxed);
        self.batch_size_sum.fetch_add(size, Ordering::Relaxed);
        self.batch_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a search operation
    pub fn record_search(&self) {
        self.total_searches.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Get current throughput (records per second)
    pub fn get_throughput(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.total_inserts.load(Ordering::Relaxed) as f64 / elapsed
        } else {
            0.0
        }
    }
    
    /// Get average insert latency in microseconds
    pub fn get_avg_latency_micros(&self) -> f64 {
        let sum = self.insert_latency_sum.load(Ordering::Relaxed);
        let count = self.insert_latency_count.load(Ordering::Relaxed);
        if count > 0 {
            sum as f64 / count as f64
        } else {
            0.0
        }
    }
    
    /// Get average batch size
    pub fn get_avg_batch_size(&self) -> f64 {
        let sum = self.batch_size_sum.load(Ordering::Relaxed);
        let count = self.batch_count.load(Ordering::Relaxed);
        if count > 0 {
            sum as f64 / count as f64
        } else {
            0.0
        }
    }
    
    /// Get total operations
    pub fn get_total_inserts(&self) -> usize {
        self.total_inserts.load(Ordering::Relaxed)
    }
    
    pub fn get_total_searches(&self) -> usize {
        self.total_searches.load(Ordering::Relaxed)
    }
    
    pub fn get_total_batches(&self) -> usize {
        self.total_batches.load(Ordering::Relaxed)
    }
    
    /// Print current metrics
    pub fn print_summary(&self) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║                    MÉTRICAS DE RENDIMIENTO                     ║");
        println!("╚════════════════════════════════════════════════════════════════╝");
        println!("⏱️  Tiempo transcurrido: {:.2}s", elapsed);
        println!("📊 Total Inserts: {}", self.get_total_inserts());
        println!("🔍 Total Searches: {}", self.get_total_searches());
        println!("📦 Total Batches: {}", self.get_total_batches());
        println!("🚀 Throughput: {:.2} RPS", self.get_throughput());
        println!("⚡ Latencia Promedio: {:.2}µs ({:.4}ms)", 
            self.get_avg_latency_micros(),
            self.get_avg_latency_micros() / 1000.0
        );
        println!("📈 Tamaño Promedio de Batch: {:.2}", self.get_avg_batch_size());
        println!();
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
