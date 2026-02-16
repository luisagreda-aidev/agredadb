use agredadb_server::server::AgredaService;
use agredadb_dbms::agreda_proto::{BlobChunk, QueryRequest, LoginRequest, CreateUserRequest};
use agredadb_dbms::agreda_proto::agreda_server::Agreda;
use tonic::Request;
use std::time::Instant;
use tokio_stream::iter;

#[tokio::test]
async fn test_hybrid_interference() -> Result<(), Box<dyn std::error::Error>> {
    let service = AgredaService::new().await;
    
    // Auth
    service.create_user(Request::new(CreateUserRequest {
        username: "blob_user".into(), password: "pass".into(), tenant: "hybrid".into()
    })).await?;
    let login = service.login(Request::new(LoginRequest { username: "blob_user".into(), password: "pass".into() })).await?.into_inner();
    let token = login.token;

    // 1. Iniciar subida masiva de BLOB (100 MB) en background
    let token_clone = token.clone();
    let service_clone = AgredaService::new().await; // Clon para concurrencia real
    let blob_handle = tokio::spawn(async move {
        let chunk = BlobChunk { data: vec![0u8; 1024 * 1024], blob_id: "test_blob".into() };
        let stream = iter(vec![chunk; 100]); // 100 MB
        let mut req = Request::new(stream);
        req.metadata_mut().insert("authorization", format!("Bearer {}", token_clone).parse().unwrap());
        service_clone.upload_blob(req).await.unwrap();
    });

    // 2. Medir latencia SQL mientras el BLOB se sube
    let mut sql_req = Request::new(QueryRequest { sql: "SELECT 1+1".into() });
    sql_req.metadata_mut().insert("authorization", format!("Bearer {}", token).parse()?);
    
    let start = Instant::now();
    service.query(sql_req).await?;
    let duration = start.elapsed();

    println!("📦 Latencia SQL bajo carga de Blobs: {:?}", duration);
    
    blob_handle.await?;
    println!("✅ Prueba de interferencia híbrida completada.");
    Ok(())
}
