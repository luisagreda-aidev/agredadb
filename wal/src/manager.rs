//! WAL manager implementation

use crate::entry::WalEntry;
use crate::error::Result;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Sync policy for WAL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPolicy {
    /// Sync after every write
    EveryWrite,
    /// Sync after N writes
    Batched(usize),
    /// Never sync (for testing)
    Never,
    /// Adaptive: sync based on batch size and time
    /// Parameters: (min_batch_size, max_delay_micros)
    Adaptive(usize, u64),
}

/// WAL Manager (Async Tokio Version)
pub struct WalManager {
    file: Arc<Mutex<BufWriter<File>>>,
    path: String,
    sync_policy: SyncPolicy,
    write_count: Arc<Mutex<usize>>,
    last_sync: Arc<Mutex<std::time::Instant>>,
}

impl WalManager {
    /// Create new WAL manager
    pub async fn new<P: AsRef<Path>>(path: P, sync_policy: SyncPolicy) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        Ok(Self {
            file: Arc::new(Mutex::new(BufWriter::new(file))),
            path: path_str,
            sync_policy,
            write_count: Arc::new(Mutex::new(0)),
            last_sync: Arc::new(Mutex::new(std::time::Instant::now())),
        })
    }

    /// Append entry to WAL
    pub async fn append(&self, entry: WalEntry) -> Result<u64> {
        let bytes = entry.to_bytes()?;
        let len = bytes.len() as u32;

        let mut file = self.file.lock().await;
        
        // Write length prefix
        file.write_all(&len.to_le_bytes()).await?;
        
        // Write entry
        file.write_all(&bytes).await?;

        // Update write count
        let mut count = self.write_count.lock().await;
        *count += 1;

        // Sync if needed (Optimistic flush, real sync happens in explicit sync call or policy)
        match self.sync_policy {
            SyncPolicy::EveryWrite => {
                file.flush().await?;
                // Note: file.sync_all() is slow, we usually batch it.
                // But for strict policy we might want it. 
                // However, in Group Commit architecture, we call sync() explicitly.
            }
            SyncPolicy::Batched(n) if *count >= n => {
                file.flush().await?;
                *count = 0;
            }
            SyncPolicy::Adaptive(min_batch, max_delay_micros) => {
                let last_sync = self.last_sync.lock().await;
                let elapsed_micros = last_sync.elapsed().as_micros() as u64;
                drop(last_sync);
                
                // Sync if we have enough writes OR if too much time has passed
                if *count >= min_batch || elapsed_micros >= max_delay_micros {
                    file.flush().await?;
                    *count = 0;
                }
            }
            _ => {}
        }

        Ok((len + 4) as u64)
    }

    /// Sync WAL to disk (The expensive fsync)
    pub async fn sync(&self) -> Result<()> {
        let mut file = self.file.lock().await;
        file.flush().await?;
        file.get_ref().sync_all().await?;
        
        // Update last sync time
        let mut last_sync = self.last_sync.lock().await;
        *last_sync = std::time::Instant::now();
        
        Ok(())
    }

    /// Replay WAL entries
    pub async fn replay(&self) -> Result<Vec<WalEntry>> {
        use tokio::io::AsyncReadExt;
        
        let mut file = File::open(&self.path).await?;
        let mut entries = Vec::new();

        loop {
            // Read length prefix
            let mut len_bytes = [0u8; 4];
            match file.read_exact(&mut len_bytes).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            let len = u32::from_le_bytes(len_bytes) as usize;

            // Read entry
            let mut entry_bytes = vec![0u8; len];
            file.read_exact(&mut entry_bytes).await?;

            // Deserialize
            let entry = WalEntry::from_bytes(&entry_bytes)?;
            entries.push(entry);
        }

        Ok(entries)
    }
}