use agredadb_storage::RawBlockManager;
use std::time::Instant;
use aligned_vec::{AVec, ConstAlign};

#[test]
fn test_io_performance() -> Result<(), Box<dyn std::error::Error>> {
    let path = "test_io_saturation.bin";
    // Crear un archivo de 1GB para la prueba
    println!("📦 Creando archivo de prueba de 1GB...");
    let file = std::fs::File::create(path)?;
    file.set_len(1024 * 1024 * 1024)?;
    drop(file);

    let manager = RawBlockManager::open(path)?;
    let block_size = 4096;
    let total_blocks = (1024 * 1024 * 1024) / block_size;
    
    println!("🚀 Iniciando lectura secuencial con Kernel Bypass...");
    let start = Instant::now();
    
    for i in 0..total_blocks {
        let _ = manager.read_block(i as u64 * block_size as u64, block_size)?;
    }

    let duration = start.elapsed();
    let gb_per_sec = 1.0 / duration.as_secs_f64();
    
    println!("⏱️ Tiempo total: {:?}", duration);
    println!("📊 Velocidad de Lectura Real: {:.2} GB/s", gb_per_sec);
    
    std::fs::remove_file(path)?;
    Ok(())
}
