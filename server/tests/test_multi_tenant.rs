use agredadb_server::server::AgredaService;
use agredadb_dbms::agreda_proto::{InsertRequest, SearchRequest, LoginRequest, CreateUserRequest};
use agredadb_dbms::agreda_proto::agreda_server::Agreda;
use tonic::Request;

#[tokio::test]
async fn test_multi_tenant_isolation() -> Result<(), Box<dyn std::error::Error>> {
    let service = AgredaService::new().await;
    
    // 1. Crear usuarios para Tenant A y Tenant B
    service.create_user(Request::new(CreateUserRequest {
        username: "user_a".into(), password: "pass".into(), tenant: "tenant_a".into()
    })).await?;
    
    service.create_user(Request::new(CreateUserRequest {
        username: "user_b".into(), password: "pass".into(), tenant: "tenant_b".into()
    })).await?;

    // 2. Obtener tokens
    let login_a = service.login(Request::new(LoginRequest { username: "user_a".into(), password: "pass".into() })).await?.into_inner();
    let login_b = service.login(Request::new(LoginRequest { username: "user_b".into(), password: "pass".into() })).await?.into_inner();
    
    // 3. Insertar dato como Tenant A
    let mut req_a = Request::new(InsertRequest {
        table: "secure_table".into(), 
        id: "secret_a".into(), 
        json_data: r#"{"data": "A-ONLY", "vector": [0.1, 0.1]}"#.into()
    });
    req_a.metadata_mut().insert("authorization", format!("Bearer {}", login_a.token).parse()?);
    service.insert(req_a).await?;

    // 4. Intentar buscar como Tenant B
    let mut search_b = Request::new(SearchRequest {
        table: "secure_table".into(), 
        vector: vec![0.1; 128], 
        limit: 10
    });
    search_b.metadata_mut().insert("authorization", format!("Bearer {}", login_b.token).parse()?);
    let results_b = service.search(search_b).await?.into_inner();

    // 5. Validar aislamiento
    println!("🔒 Resultados encontrados por Tenant B: {}", results_b.results.len());
    assert_eq!(results_b.results.len(), 0, "Tenant B NO debería ver datos de Tenant A");
    
    println!("✅ Aislamiento Multi-tenant verificado exitosamente.");
    Ok(())
}
