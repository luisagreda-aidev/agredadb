//! # AgredaDB Write-Ahead Log (WAL)
//!
//! ACID-compliant Write-Ahead Log implementation with io_uring support.
//!
//! ## Features
//!
//! - ACID guarantees
//! - Zero data loss
//! - Fast recovery
//! - Sequential writes optimized for NVMe
//!
//! ## Example
//!
//! ```rust,no_run
//! use agredadb_wal::{WalManager, WalEntry, SyncPolicy};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create WAL manager
//! let mut wal = WalManager::new("./wal.log", SyncPolicy::EveryWrite).await?;
//!
//! // Append entry
//! let entry = WalEntry::Insert {
//!     table: "users".to_string(),
//!     data: vec![1, 2, 3],
//! };
//! wal.append(entry).await?;
//!
//! // Sync to disk
//! wal.sync().await?;
//! # Ok(())
//! # }
//! ```

pub mod manager;
pub mod entry;
pub mod error;

pub use manager::{WalManager, SyncPolicy};
pub use entry::WalEntry;
pub use error::{WalError, Result};

#[cfg(test)]
mod tests {
    #[test]
    fn test_basic() {
        // Basic test to ensure module compiles
        assert!(true);
    }
}
