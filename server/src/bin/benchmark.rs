use std::time::Instant;
use tokio::task;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tonic::Request;

pub mod agreda_proto {
    tonic::include_proto!("agreda");
}

use agreda_proto::agreda_client::AgredaClient;
use agreda_proto::InsertBatchRequest;

const TOTAL_CLIENTS: usize = 50; 
const BATCHES_PER_CLIENT: usize = 200; // 200 envíos
const RECORDS_PER_BATCH: usize = 100;  // 100 registros por envío
// Total = 50 * 200 * 100 = 1,000,000 Registros (1 Millón)

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("INICIANDO STRESS TEST (MODO BATCH - SCYLLA KILLER)...");
    println!("Objetivo: 127.0.0.1:50051");
    println!("Clientes: {}", TOTAL_CLIENTS);
    println!("Registros por Batch: {}", RECORDS_PER_BATCH);
    println!("Total Registros a Inyectar: 1,000,000");
    
    let counter = Arc::new(AtomicUsize::new(0));
    let start_time = Instant::now();
    
    let mut handles = Vec::new();

    // Pre-generar batch data para no medir tiempo de CPU del cliente
    let sample_json = "{\"uuid\": \"abc-123\", \"vector\": [0.1, 0.2, 0.9]}".to_string();
    let batch_payload = vec![sample_json; RECORDS_PER_BATCH];

    for i in 0..TOTAL_CLIENTS {
        let counter_clone = counter.clone();
        let payload = batch_payload.clone();
        
        handles.push(task::spawn(async move {
            let client_res = AgredaClient::connect("http://127.0.0.1:50051").await;
            
            if let Ok(mut client) = client_res {
                for _ in 0..BATCHES_PER_CLIENT {
                    let req = Request::new(InsertBatchRequest {
                        table: "bench_batch".to_string(),
                        json_data_list: payload.clone(),
                    });
                    
                    if client.insert_batch(req).await.is_ok() {
                        counter_clone.fetch_add(RECORDS_PER_BATCH, Ordering::Relaxed);
                    }
                }
            } else {
                eprintln!("Fallo conexion cliente {}", i);
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    let duration = start_time.elapsed();
    let total_records = counter.load(Ordering::Relaxed);
    let secs = duration.as_secs_f64();
    let rps = total_records as f64 / secs;

    println!("\n------------------");
    println!("Total Registros: {}", total_records);
    println!("Tiempo: {:.2}s", secs);
    println!("🚀 THROUGHPUT: {:.2} RECORDS/SEC", rps);
    println!("------------------");

    if rps > 35000.0 {
        println!("🏆 SCYLLA-DB HA SIDO SUPERADA.");
    } else {
        println!("⚠️ Aún no superamos a Scylla.");
    }

    Ok(())
}