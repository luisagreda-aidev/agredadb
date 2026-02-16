use std::time::{Instant};
use tokio::task;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tonic::Request;
use hdrhistogram::Histogram;
use parking_lot::Mutex;
use agredadb_dbms::agreda_proto;

use agreda_proto::agreda_client::AgredaClient;
use agreda_proto::InsertRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let total_records = 20_000; 
    let num_clients = 40; // Presión moderada pero constante

    println!("🔬 BALANCED PERFORMANCE TEST: AgredaDB 1-a-1");
    println!("-------------------------------------------------------------");
    println!("⚙️ Mode: SINGLE RECORD INSERT (1 Req = 1 Rec)");
    println!("💾 Consistency: WAL Sync Enabled (ACID)");
    println!("🧵 Concurrency: {} Clients", num_clients);
    println!("-------------------------------------------------------------\
");

    let counter = Arc::new(AtomicUsize::new(0));
    let histogram = Arc::new(Mutex::new(Histogram::<u64>::new_with_bounds(1, 1000000, 3).unwrap()));
    let start_time = Instant::now();

    let mut handles = Vec::new();

    for i in 0..num_clients {
        let counter_clone = counter.clone();
        let hist_clone = histogram.clone();
        
        handles.push(task::spawn(async move {
            if let Ok(mut client) = AgredaClient::connect("http://127.0.0.1:50051").await {
                while counter_clone.load(Ordering::Relaxed) < total_records {
                    let op_start = Instant::now();
                    let req = Request::new(InsertRequest {
                        table: "balanced_test".to_string(),
                        id: format!("id_{}_{}", i, rand::random::<u32>()),
                        json_data: "{\"val\": 1.0}".to_string(),
                    });
                    
                    if client.insert(req).await.is_ok() {
                        let latency = op_start.elapsed().as_micros() as u64;
                        counter_clone.fetch_add(1, Ordering::Relaxed);
                        let _ = hist_clone.lock().record(latency);
                    } else {
                        // Si falla una conexión, salimos del hilo para no falsear m´étricas
                        break;
                    }
                }
            }
        }));
    }

    for h in handles { let _ = h.await; }

    let duration = start_time.elapsed();
    let total_done = counter.load(Ordering::Relaxed);
    let rps = total_done as f64 / duration.as_secs_f64();
    let hist = histogram.lock();

    println!("📊 FINAL RESULTS:");
    println!("-----------------------------------------------------------------------");
    println!("Throughput (RPS)| {:<13.0}", rps);
    println!("Avg Latency     | {:<13.2} ms", (hist.mean() / 1000.0));
    println!("p99 Latency     | {:<13.2} ms", (hist.value_at_quantile(0.99) as f64 / 1000.0));
    println!("-----------------------------------------------------------------------");
    
    Ok(())
}