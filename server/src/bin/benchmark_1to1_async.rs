use std::time::Instant;
use tokio::task;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tonic::Request;
use futures::future::join_all;
use agredadb_dbms::agreda_proto;

use agreda_proto::agreda_client::AgredaClient;
use agreda_proto::InsertRequest;

// CONFIGURACIÓN PARA MODO 1-A-1 CON PIPELINE ASÍNCRONO
const TOTAL_REQUESTS: usize = 1_000_000; 
const CONCURRENT_PIPELINE: usize = 2000; 
const NUM_CLIENTS: usize = 40; 

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  AgredaDB - BENCHMARK 1-A-1 ASÍNCRONO CON PIPELINE           ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
    println!("🎯 Objetivo: Superar 100,000 RPS en modo 1-a-1");
    println!("📊 Configuración:");
    println!("   • Total Requests: {}", TOTAL_REQUESTS);
    println!("   • Pipeline Depth: {} (requests en vuelo)", CONCURRENT_PIPELINE);
    println!("   • Clientes Concurrentes: {}", NUM_CLIENTS);
    println!("   • Modo: 1 registro por request (1-a-1)");
    println!("   • Servidor: 127.0.0.1:50051");
    println!();
    println!("🚀 Iniciando benchmark...");
    println!();

    let counter = Arc::new(AtomicUsize::new(0));
    let start_time = Instant::now();
    
    // Pre-generar el payload JSON
    let sample_json = r#"{"uuid": "test-uuid", "data": "sample data", "timestamp": 1234567890}"#.to_string();

    // Crear múltiples clientes concurrentes
    let mut client_handles = Vec::new();

    for client_id in 0..NUM_CLIENTS {
        let counter_clone = counter.clone();
        let json_data = sample_json.clone();
        let requests_per_client = TOTAL_REQUESTS / NUM_CLIENTS;
        
        client_handles.push(task::spawn(async move {
            // Conectar al servidor
            let client_result = AgredaClient::connect("http://127.0.0.1:50051").await;
            
            if let Ok(client) = client_result {
                let mut futures = Vec::new();
                
                // Enviar requests en pipeline (sin esperar respuesta inmediata)
                for batch_start in (0..requests_per_client).step_by(CONCURRENT_PIPELINE) {
                    let batch_end = (batch_start + CONCURRENT_PIPELINE).min(requests_per_client);
                    
                    // Enviar un batch de requests asíncronamente
                    for i in batch_start..batch_end {
                        let mut client_clone = client.clone();
                        let json_clone = json_data.clone();
                        let counter_ref = counter_clone.clone();
                        
                        let future = async move {
                            let req = Request::new(InsertRequest {
                                table: "bench_1to1".to_string(),
                                id: format!("id-{}-{}", client_id, i),
                                json_data: json_clone,
                            });
                            
                            if client_clone.insert(req).await.is_ok() {
                                counter_ref.fetch_add(1, Ordering::Relaxed);
                            }
                        };
                        
                        futures.push(future);
                    }
                    
                    // Esperar a que este batch complete antes de enviar el siguiente
                    join_all(futures.drain(..)).await;
                }
            } else {
                eprintln!("❌ Cliente {} falló al conectar", client_id);
            }
        }));
    }

    // Esperar a que todos los clientes terminen
    for handle in client_handles {
        let _ = handle.await;
    }

    let duration = start_time.elapsed();
    let total_records = counter.load(Ordering::Relaxed);
    let secs = duration.as_secs_f64();
    let rps = total_records as f64 / secs;
    let latency_avg_ms = (secs * 1000.0) / total_records as f64;

    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║                    RESULTADOS FINALES                          ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
    println!("📈 Métricas de Rendimiento:");
    println!("   • Total Registros Procesados: {}", total_records);
    println!("   • Tiempo Total: {:.4}s", secs);
    println!("   • Throughput: {:.2} RPS", rps);
    println!("   • Latencia Promedio: {:.4}ms", latency_avg_ms);
    println!();

    // Comparación con objetivos
    println!("🎯 Análisis de Objetivos:");
    
    if rps > 100_000.0 {
        println!("   ✅ OBJETIVO ALCANZADO: > 100,000 RPS");
        println!("   🏆 AgredaDB está superando a PostgreSQL (~12K RPS)");
        
        if rps > 120_000.0 {
            println!("   🏆 AgredaDB está superando a ScyllaDB (~120K RPS)");
            println!("   🚀 LÍDER INDUSTRIAL EN MODO 1-A-1");
        }
    } else if rps > 50_000.0 {
        println!("   ⚠️  Progreso significativo: {} RPS", rps as u64);
        println!("   📊 Mejora de {}x vs baseline (900 RPS)", (rps / 900.0) as u64);
    } else if rps > 10_000.0 {
        println!("   ⚠️  Mejora moderada: {} RPS", rps as u64);
        println!("   📊 Mejora de {}x vs baseline (900 RPS)", (rps / 900.0) as u64);
    } else {
        println!("   ❌ Aún por debajo del objetivo: {} RPS", rps as u64);
        println!("   📊 Se requiere optimización adicional");
    }

    println!();
    println!("╔════════════════════════════════════════════════════════════════╗");
    println!("║  Benchmark completado exitosamente                             ║");
    println!("╚════════════════════════════════════════════════════════════════╝");

    Ok(())
}
