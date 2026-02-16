use agredadb_dbms::agreda_proto::SearchResult;
use crate::compute::ComputeEngine;
use crate::metrics::Metrics;
use crate::durability::{DurabilityMode, get_durability_mode};
use agredadb_storage::{RawBlockManager, TurboCompressor};
use agredadb_wal::{WalManager, WalEntry, SyncPolicy};
use agredadb_dbms::{DiskAnnIndex, InternalCommand};
use arrow::record_batch::RecordBatch;
use arrow::array::{StringArray, FixedSizeListArray};
use arrow::compute::concat_batches;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_DRAIN_SIZE: usize = 10_000;
const VECTOR_DIM: usize = 128;
const ADAPTIVE_TIMEOUT_MICROS: u64 = 50;
const MIN_BATCH_FOR_IMMEDIATE_PROCESS: usize = 10;
const MAX_MEMORY_BATCHES: usize = 100;
const MAX_MEMORY_ROWS: usize = 1_000_000;
const FLUSH_THRESHOLD_ROWS: usize = 500_000;

fn build_schema() -> Arc<arrow::datatypes::Schema> {
    Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Utf8, false),
        arrow::datatypes::Field::new("metadata", arrow::datatypes::DataType::Utf8, false),
        arrow::datatypes::Field::new("vector", arrow::datatypes::DataType::FixedSizeList(
            Arc::new(arrow::datatypes::Field::new("item", arrow::datatypes::DataType::Float32, true)),
            VECTOR_DIM as i32
        ), false),
    ]))
}

fn parse_vector_from_json(json: &str) -> Vec<f32> {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json) {
        if let Some(vector) = parsed.get("vector") {
            if let Some(arr) = vector.as_array() {
                let v: Vec<f32> = arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect();
                if v.len() == VECTOR_DIM {
                    return v;
                }
            }
        }
        if let Some(vector) = parsed.get("embedding") {
            if let Some(arr) = vector.as_array() {
                let v: Vec<f32> = arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect();
                if v.len() == VECTOR_DIM {
                    return v;
                }
            }
        }
    }
    vec![0.0f32; VECTOR_DIM]
}

pub struct Shard {
    id: usize,
    _storage: Arc<RawBlockManager>,
    _compressor: TurboCompressor,
    _disk_index: Arc<DiskAnnIndex>,
    wal: Option<Arc<Mutex<WalManager>>>,
    receiver: flume::Receiver<InternalCommand>,
    memory_table: Arc<Mutex<Vec<RecordBatch>>>,
    _block_index: Arc<Mutex<HashMap<u64, u64>>>,
    _next_physical_offset: Arc<Mutex<u64>>,
    metrics: Metrics,
    durability_mode: DurabilityMode,
    schema: Arc<arrow::datatypes::Schema>,
    data_path: String,
    flushed_files: Arc<Mutex<Vec<String>>>,
}

impl Shard {
    pub async fn new(id: usize, receiver: flume::Receiver<InternalCommand>) -> Self {
        Self::new_with_mode(id, receiver, get_durability_mode()).await
    }

    pub async fn new_with_mode(
        id: usize,
        receiver: flume::Receiver<InternalCommand>,
        durability_mode: DurabilityMode
    ) -> Self {
        let data_path = format!("./data/shard_{}", id);
        let _ = std::fs::create_dir_all(&data_path);
        let block_file = format!("{}/data.bin", data_path);
        if !std::path::Path::new(&block_file).exists() {
            std::fs::File::create(&block_file).unwrap();
        }
        let storage = Arc::new(RawBlockManager::open(&block_file).expect("Failed to open storage"));

        let (wal, recovered_jsons) = if durability_mode.uses_wal() {
            let wal_path = format!("{}/wal.log", data_path);

            // Replay existing WAL for crash recovery
            let mut recovered = Vec::new();
            if std::path::Path::new(&wal_path).exists() {
                if let Ok(replay_mgr) = WalManager::new(&wal_path, SyncPolicy::Never).await {
                    match replay_mgr.replay().await {
                        Ok(entries) => {
                            for entry in entries {
                                if let WalEntry::Insert { data, .. } = entry {
                                    match String::from_utf8(data) {
                                        Ok(json) => recovered.push(json),
                                        Err(e) => log::warn!("Shard {}: skipping non-UTF8 WAL entry: {}", id, e),
                                    }
                                }
                            }
                            // Truncate WAL after successful replay to avoid duplicate recovery
                            if let Err(e) = std::fs::OpenOptions::new()
                                .write(true)
                                .truncate(true)
                                .open(&wal_path)
                            {
                                log::warn!("Shard {}: failed to truncate WAL after replay: {}", id, e);
                            }
                        }
                        Err(e) => log::warn!("Shard {}: WAL replay failed: {}", id, e),
                    }
                }
                if !recovered.is_empty() {
                    println!("  ♻️  Shard {}: Recovered {} entries from WAL", id, recovered.len());
                }
            }

            let sync_policy = match durability_mode {
                DurabilityMode::Async => SyncPolicy::Adaptive(50, 1000),
                DurabilityMode::Strict => SyncPolicy::EveryWrite,
                DurabilityMode::Memory => SyncPolicy::Never,
            };
            let wal_manager = WalManager::new(&wal_path, sync_policy)
                .await
                .expect("Failed to init WAL");
            (Some(Arc::new(Mutex::new(wal_manager))), recovered)
        } else {
            (None, Vec::new())
        };

        let index_path = format!("{}/index.vamana", data_path);
        let disk_index = Arc::new(DiskAnnIndex::new(&index_path, storage.clone()));

        if id == 0 {
            println!("╔════════════════════════════════════════════════════════════════╗");
            println!("║  🚀 AgredaDB Shard {} - Durability Mode: {}              ", id, durability_mode);
            println!("╠════════════════════════════════════════════════════════════════╣");
            println!("║  {}", durability_mode.description());
            println!("║  Expected Throughput: {}", durability_mode.expected_throughput());
            println!("║  Durability: {}", durability_mode.durability_guarantee());
            println!("╚════════════════════════════════════════════════════════════════╝");
        }

        // Scan for existing flushed Arrow IPC files from previous runs
        let mut existing_flushed = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&data_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("arrow") {
                    existing_flushed.push(path.to_string_lossy().to_string());
                }
            }
        }
        if !existing_flushed.is_empty() {
            println!("  💾 Shard {}: Found {} flushed Arrow files on disk", id, existing_flushed.len());
        }

        let mut shard = Self {
            id,
            _storage: storage,
            _compressor: TurboCompressor::new(),
            _disk_index: disk_index,
            wal,
            receiver,
            memory_table: Arc::new(Mutex::new(Vec::new())),
            _block_index: Arc::new(Mutex::new(HashMap::new())),
            _next_physical_offset: Arc::new(Mutex::new(0)),
            metrics: Metrics::new(),
            durability_mode,
            schema: build_schema(),
            data_path: data_path.clone(),
            flushed_files: Arc::new(Mutex::new(existing_flushed)),
        };

        if !recovered_jsons.is_empty() {
            shard.insert_to_memory(recovered_jsons).await;
        }

        shard
    }

    pub async fn run(&mut self) {
        let mut batch_buffer = Vec::with_capacity(MAX_DRAIN_SIZE);

        loop {
            let first_cmd = match self.receiver.recv_async().await {
                Ok(cmd) => cmd,
                Err(_) => break,
            };
            batch_buffer.push(first_cmd);

            for _ in 0..MAX_DRAIN_SIZE {
                match self.receiver.try_recv() {
                    Ok(cmd) => batch_buffer.push(cmd),
                    Err(_) => break,
                }
            }

            if batch_buffer.len() < MIN_BATCH_FOR_IMMEDIATE_PROCESS {
                let timeout = Duration::from_micros(ADAPTIVE_TIMEOUT_MICROS);
                let deadline = tokio::time::Instant::now() + timeout;

                while tokio::time::Instant::now() < deadline
                    && batch_buffer.len() < MIN_BATCH_FOR_IMMEDIATE_PROCESS {
                    match tokio::time::timeout_at(deadline, self.receiver.recv_async()).await {
                        Ok(Ok(cmd)) => batch_buffer.push(cmd),
                        _ => break,
                    }
                }
            }

            self.process_batch(&mut batch_buffer).await;
            self.metrics.record_batch(batch_buffer.len());
            batch_buffer.clear();
        }
    }

    async fn process_batch(&mut self, commands: &mut Vec<InternalCommand>) {
        let process_start = Instant::now();
        let mut inserts = Vec::new();
        let mut responders = Vec::new();

        for cmd in commands.drain(..) {
            match cmd {
                InternalCommand::Insert { json, responder, .. } => {
                    inserts.push(json);
                    if let Some(r) = responder { responders.push(r); }
                },
                InternalCommand::InsertBatch { jsons, responder, .. } => {
                    inserts.extend(jsons);
                    if let Some(r) = responder { responders.push(r); }
                },
                InternalCommand::Search { vector, limit, responder, .. } => {
                    let results = self.perform_hybrid_search(vector, limit).await;
                    let _ = responder.send(results);
                    self.metrics.record_search();
                },
                InternalCommand::Query { responder, .. } => {
                    let batches = self.memory_table.lock().await.clone();
                    for batch in batches {
                        if let Ok(projected) = batch.project(&[0, 1]) {
                            let _ = responder.send(projected);
                        }
                    }
                    // Also include flushed Arrow files for SQL queries
                    let flushed = self.flushed_files.lock().await.clone();
                    for path in flushed.iter() {
                        if let Ok(file) = std::fs::File::open(path) {
                            if let Ok(reader) = arrow::ipc::reader::FileReader::try_new(file, None) {
                                for batch_result in reader {
                                    if let Ok(batch) = batch_result {
                                        if let Ok(projected) = batch.project(&[0, 1]) {
                                            let _ = responder.send(projected);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if !inserts.is_empty() {
            let insert_count = inserts.len();

            let success = match self.durability_mode {
                DurabilityMode::Memory => {
                    self.insert_to_memory(inserts).await;
                    true
                },
                DurabilityMode::Async | DurabilityMode::Strict => {
                    if let Some(ref wal) = self.wal {
                        let mut wal_guard = wal.lock().await;
                        for json in &inserts {
                            let _ = wal_guard.append(WalEntry::Insert {
                                table: "default".into(),
                                data: json.as_bytes().to_vec()
                            }).await;
                        }

                        let sync_res = if self.durability_mode == DurabilityMode::Strict {
                            wal_guard.sync().await
                        } else {
                            Ok(())
                        };

                        drop(wal_guard);

                        if sync_res.is_ok() {
                            self.insert_to_memory(inserts).await;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            };

            for r in responders { let _ = r.send(success); }

            if success {
                let latency = process_start.elapsed();
                self.metrics.record_insert(insert_count, latency);
            }
        }
    }

    pub async fn insert_to_memory(&mut self, jsons: Vec<String>) {
        let count = jsons.len();
        if count == 0 { return; }

        let mut ids = Vec::with_capacity(count);
        let mut metadatas = Vec::with_capacity(count);
        let mut all_vectors = Vec::with_capacity(count * VECTOR_DIM);

        for j in &jsons {
            ids.push(uuid::Uuid::new_v4().to_string());
            metadatas.push(j.clone());
            all_vectors.extend_from_slice(&parse_vector_from_json(j));
        }

        let batch = RecordBatch::try_new(self.schema.clone(), vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(metadatas)),
            Arc::new(FixedSizeListArray::new(
                Arc::new(arrow::datatypes::Field::new("item", arrow::datatypes::DataType::Float32, true)),
                VECTOR_DIM as i32,
                Arc::new(arrow::array::Float32Array::from(all_vectors)),
                None
            )),
        ]).unwrap();

        let mut table = self.memory_table.lock().await;
        table.push(batch);

        // Compact when too many small batches accumulate
        if table.len() > MAX_MEMORY_BATCHES {
            if let Ok(merged) = concat_batches(&self.schema, table.iter()) {
                table.clear();

                if merged.num_rows() > FLUSH_THRESHOLD_ROWS {
                    // Flush to Arrow IPC file on disk
                    let flush_ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    let flush_path = format!("{}/flush_{}.arrow", self.data_path, flush_ts);

                    match std::fs::File::create(&flush_path) {
                        Ok(file) => {
                            match arrow::ipc::writer::FileWriter::try_new(file, &self.schema) {
                                Ok(mut writer) => {
                                    if writer.write(&merged).is_ok() && writer.finish().is_ok() {
                                        self.flushed_files.lock().await.push(flush_path.clone());
                                        println!("  💾 Shard {}: Flushed {} rows to {}", self.id, merged.num_rows(), flush_path);
                                    } else {
                                        log::warn!("Shard {}: Failed to write Arrow IPC file, keeping in memory", self.id);
                                        table.push(merged);
                                    }
                                }
                                Err(e) => {
                                    log::warn!("Shard {}: Failed to create Arrow writer: {}, keeping in memory", self.id, e);
                                    table.push(merged);
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("Shard {}: Failed to create flush file: {}, keeping in memory", self.id, e);
                            table.push(merged);
                        }
                    }
                } else if merged.num_rows() > MAX_MEMORY_ROWS {
                    let keep = MAX_MEMORY_ROWS / 2;
                    let offset = merged.num_rows() - keep;
                    table.push(merged.slice(offset, keep));
                    log::warn!("Shard {}: memory_table exceeded {} rows, trimmed to {}", self.id, MAX_MEMORY_ROWS, keep);
                } else {
                    table.push(merged);
                }
            }
        }
    }

    async fn perform_hybrid_search(&self, vector: Vec<f32>, limit: u32) -> Vec<SearchResult> {
        let mut all_results = Vec::new();

        // Search in-memory batches
        let memory_table = self.memory_table.lock().await;
        for batch in memory_table.iter() {
            all_results.extend(ComputeEngine::vector_search(batch, &vector, limit as usize));
        }
        drop(memory_table);

        // Search flushed Arrow IPC files on disk
        let flushed = self.flushed_files.lock().await;
        for path in flushed.iter() {
            if let Ok(file) = std::fs::File::open(path) {
                if let Ok(reader) = arrow::ipc::reader::FileReader::try_new(file, None) {
                    for batch_result in reader {
                        if let Ok(batch) = batch_result {
                            all_results.extend(ComputeEngine::vector_search(&batch, &vector, limit as usize));
                        }
                    }
                }
            }
        }
        drop(flushed);

        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(limit as usize);
        all_results.into_iter().map(|p| SearchResult {
            id: p.id,
            score: p.score,
            json_data: p.data
        }).collect()
    }
}
