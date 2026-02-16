use datafusion::prelude::*;
use std::time::Instant;
use std::sync::Arc;
use arrow::array::Float64Array;
use arrow::record_batch::RecordBatch;
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::MemTable;

#[tokio::test]
async fn test_datafusion_simd_speed() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = SessionContext::new();
    
    // 10 Millones de filas para que sea un test serio
    let row_count = 10_000_000;
    let schema = Arc::new(Schema::new(vec![
        Field::new("val", DataType::Float64, false),
    ]));
    
    let values = Float64Array::from(vec![1.0; row_count]);
    let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(values)])?;
    let provider = MemTable::try_new(schema, vec![vec![batch]])?;
    
    ctx.register_table("perf_test", Arc::new(provider))?;
    
    println!("📊 Calculando SUM(val) sobre {} millones de filas (SIMD)...", row_count / 1_000_000);
    let start = Instant::now();
    let df = ctx.sql("SELECT SUM(val) FROM perf_test").await?;
    let _ = df.collect().await?;
    let duration = start.elapsed();
    
    println!("⏱️ Tiempo SQL (SIMD): {:?}", duration);
    let rows_per_sec = row_count as f64 / duration.as_secs_f64();
    println!("🚀 Throughput SQL: {:.2} Millones de filas/seg", rows_per_sec / 1_000_000.0);
    
    Ok(())
}
