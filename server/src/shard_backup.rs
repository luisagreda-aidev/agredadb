use agredadb_dbms::agreda_proto::SearchResult;
use crate::compute::ComputeEngine;
use crate::metrics::Metrics;
use crate::benchmark_config::get_benchmark_mode;
use agredadb_storage::{RawBlockManager, TurboCompressor};
use agredadb_wal::{WalManager, WalEntry, SyncPolicy};
use agredadb_dbms::{DiskAnnIndex, InternalCommand};
use arrow::record_batch::RecordBatch;
use arrow::array::{StringArray, FixedSizeListArray};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const MAX_DRAIN_SIZE: usize = 10_000;
const VECTOR_DIM: usize = 128;
const ADAPTIVE_TIMEOUT_MICROS: u64 = 50; // 50 microseconds
const MIN_BATCH_FOR_IMMEDIATE_PROCESS: usize = 10; // Process immediately if we have 10+ items

pub struct Shard {
    _id: usize,
    _storage: Arc<RawBlockManager>,
    _compressor: TurboCompressor,
    _disk_index: Arc<DiskAnnIndex>,
    wal: Arc<Mutex<WalManager>>,
    receiver: flume::Receiver<InternalCommand>,
    memory_table: Arc<Mutex<Vec<RecordBatch>>>,
    _block_index: Arc<Mutex<HashMap<u64, u64>>>,
    _next_physical_offset: Arc<Mutex<u64>>,
    metrics: Metrics,
}

impl Shard {
    pub async fn new(id: usize, receiver: flume::Receiver<InternalCommand>) -> Self {
        let data_path = format!("./data/shard_{}", id);
        let _ = std::fs::create_dir_all(&data_path);
        let block_file = format!("{}/data.bin", data_path);
        if !std::path::Path::new(&block_file).exists() { std::fs::File::create(&block_file).unwrap(); }
        let storage = Arc::new(RawBlockManager::open(&block_file).expect("Failed to open storage"));
        let wal_path = format!("{}/wal.log", data_path);
        
        // Get benchmark mode from environment or use default
        let benchmark_mode = get_benchmark_mode();
        let sync_policy = benchmark_mode.sync_policy();
        
        println!("🔧 Shard {} usando modo: {}", id, benchmark_mode.description());
        
        let wal = WalManager::new(&wal_path, sync_policy).await.expect("Failed to init WAL");
        let index_path = format!("{}/index.vamana", data_path);
        let disk_index = Arc::new(DiskAnnIndex::new(&index_path, storage.clone()));

        Self {
            _id: id, _storage: storage, _compressor: TurboCompressor::new(), _disk_index: disk_index, 
            wal: Arc::new(Mutex::new(wal)), 
            receiver,
            memory_table: Arc::new(Mutex::new(Vec::new())),
            _block_index: Arc::new(Mutex::new(HashMap::new())),
            _next_physical_offset: Arc::new(Mutex::new(0)),
            metrics: Metrics::new(),
        }
    }

    pub async fn run(&mut self) {
        let mut batch_buffer = Vec::with_capacity(MAX_DRAIN_SIZE);

        loop {
            // ADAPTIVE GROUP COMMIT STRATEGY:
            // 1. Wait for first command (blocking)
            let first_cmd = match self.receiver.recv_async().await {
                Ok(cmd) => cmd,
                Err(_) => break, 
            };
            batch_buffer.push(first_cmd);

            // 2. FAST PATH: Try to drain immediately available messages
            for _ in 0..MAX_DRAIN_SIZE {
                match self.receiver.try_recv() {
                    Ok(cmd) => {
                        batch_buffer.push(cmd);
                    }
                    Err(_) => break, 
                }
            }

            // 3. ADAPTIVE WAIT: If we only got 1 message and queue is empty,
            //    wait a tiny bit to see if more arrive (micro-batching)
            if batch_buffer.len() < MIN_BATCH_FOR_IMMEDIATE_PROCESS {
                let timeout = Duration::from_micros(ADAPTIVE_TIMEOUT_MICROS);
                let deadline = tokio::time::Instant::now() + timeout;
                
                // Try to collect more messages during the micro-timeout
                while tokio::time::Instant::now() < deadline && batch_buffer.len() < MIN_BATCH_FOR_IMMEDIATE_PROCESS {
                    match tokio::time::timeout_at(deadline, self.receiver.recv_async()).await {
                        Ok(Ok(cmd)) => batch_buffer.push(cmd),
                        _ => break,
                    }
                }
            }

            // 4. Process the batch (whether it's 1 item or 1000)
            self.process_batch(&mut batch_buffer).await;
            
            // Record metrics
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
                     for batch in batches { let _ = responder.send(batch); }
                }
            }
        }

        if !inserts.is_empty() {
            let insert_count = inserts.len();
            let mut wal = self.wal.lock().await;
            for json in &inserts {
                let _ = wal.append(WalEntry::Insert { table: "default".into(), data: json.as_bytes().to_vec() }).await;
            }
            let sync_res = wal.sync().await;
            drop(wal);

            let success = sync_res.is_ok();
            for r in responders { let _ = r.send(success); }
            if success { 
                self.insert_to_memory(inserts).await;
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
        let vectors = vec![0.0f32; count * VECTOR_DIM];
        for j in &jsons {
            ids.push(uuid::Uuid::new_v4().to_string());
            metadatas.push(j.clone());
        }
        let schema = Arc::new(arrow::datatypes::Schema::new(vec![
            arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Utf8, false),
            arrow::datatypes::Field::new("metadata", arrow::datatypes::DataType::Utf8, false),
            arrow::datatypes::Field::new("vector", arrow::datatypes::DataType::FixedSizeList(Arc::new(arrow::datatypes::Field::new("item", arrow::datatypes::DataType::Float32, true)), VECTOR_DIM as i32), false),
        ]));
        let batch = RecordBatch::try_new(schema, vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(metadatas)),
            Arc::new(FixedSizeListArray::new(Arc::new(arrow::datatypes::Field::new("item", arrow::datatypes::DataType::Float32, true)), VECTOR_DIM as i32, Arc::new(arrow::array::Float32Array::from(vectors)), None)),
        ]).unwrap();
        self.memory_table.lock().await.push(batch);
    }

    async fn perform_hybrid_search(&self, vector: Vec<f32>, limit: u32) -> Vec<SearchResult> {
        let mut all_results = Vec::new();
        let memory_table = self.memory_table.lock().await;
        for batch in memory_table.iter() {
            all_results.extend(ComputeEngine::vector_search(batch, &vector, limit as usize));
        }
        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(limit as usize);
        all_results.into_iter().map(|p| SearchResult { id: p.id, score: p.score, json_data: p.data }).collect()
    }
}