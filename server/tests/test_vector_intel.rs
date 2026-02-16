use agredadb_server::compute::ComputeEngine;
use arrow::record_batch::RecordBatch;
use arrow::array::{StringArray, FixedSizeListArray, Float32Array};
use arrow::datatypes::{DataType, Field, Schema};
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_vector_search_speed() {
    let row_count = 100_000;
    let dim = 128;
    
    // Crear batch con 100k vectores
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("metadata", DataType::Utf8, false),
        Field::new("vector", DataType::FixedSizeList(
            Arc::new(Field::new("item", DataType::Float32, true)),
            dim as i32
        ), false),
    ]));

    let ids = StringArray::from(vec!["id"; row_count]);
    let metadata = StringArray::from(vec!["meta"; row_count]);
    let values = Float32Array::from(vec![0.5; row_count * dim]);
    let vectors = FixedSizeListArray::new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        dim as i32,
        Arc::new(values),
        None
    );

    let batch = RecordBatch::try_new(schema, vec![
        Arc::new(ids),
        Arc::new(metadata),
        Arc::new(vectors),
    ]).unwrap();

    let query_vector = vec![0.5; dim];
    
    println!("🧠 Iniciando búsqueda vectorial sobre 100k registros...");
    let start = Instant::now();
    let results = ComputeEngine::vector_search(&batch, &query_vector, 10);
    let duration = start.elapsed();
    
    println!("⏱️ Tiempo de búsqueda: {:?}", duration);
    println!("✅ Encontrados {} resultados.", results.len());
}
