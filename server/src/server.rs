use tonic::{Request, Response, Status, Streaming};
use std::pin::Pin;
use std::sync::Arc;
use crate::shard::Shard;
use agredadb_dbms::InternalCommand;
use agredadb_dbms::agreda_proto::agreda_server::{Agreda, AgredaServer};
use agredadb_dbms::agreda_proto::*;
use agredadb_dbms::{SqlEngine, AgredaTableProvider};
use agredadb_security::{SecurityManager, Claims};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use tokio_stream::StreamExt;

fn inject_tenant(json: &str, tenant: &str) -> String {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json) {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("_tenant".to_string(), serde_json::Value::String(tenant.to_string()));
            return serde_json::to_string(&value).unwrap_or_else(|_| json.to_string());
        }
    }
    serde_json::json!({"_data": json, "_tenant": tenant}).to_string()
}

pub struct AgredaService {
    shards: Vec<flume::Sender<InternalCommand>>,
    num_shards: usize,
    security: Arc<SecurityManager>,
    sql_engine: Arc<SqlEngine>,
    blob_dir: String,
}

impl AgredaService {
    pub async fn new() -> Self {
        let jwt_secret = std::env::var("AGREDA_JWT_SECRET").unwrap_or_else(|_| {
            log::warn!("AGREDA_JWT_SECRET not set, using default dev secret");
            "agreda_dev_secret_change_in_production".to_string()
        });

        let security = Arc::new(SecurityManager::new(&jwt_secret));

        let num_shards = num_cpus::get();
        let mut shard_senders = Vec::new();
        for i in 0..num_shards {
            let (tx, rx) = flume::unbounded();
            let mut shard = Shard::new(i, rx).await;
            tokio::spawn(async move { shard.run().await; });
            shard_senders.push(tx);
        }

        let sql_schema = Arc::new(arrow::datatypes::Schema::new(vec![
            arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Utf8, false),
            arrow::datatypes::Field::new("metadata", arrow::datatypes::DataType::Utf8, false),
        ]));
        let sql_engine = SqlEngine::new();
        let provider = Arc::new(AgredaTableProvider::new(sql_schema, shard_senders.clone()));
        sql_engine.register_native_table("data", provider).expect("Failed to register SQL table");
        let sql_engine = Arc::new(sql_engine);

        let blob_dir = "./data/blobs".to_string();
        let _ = std::fs::create_dir_all(&blob_dir);

        println!("🔒 Security: JWT authentication enabled");
        println!("📊 SQL Engine: DataFusion registered table 'data'");
        println!("📦 Blob Storage: {}", blob_dir);

        Self { shards: shard_senders, num_shards, security, sql_engine, blob_dir }
    }

    fn get_shard(&self, id: &str) -> usize {
        let mut s = DefaultHasher::new();
        id.hash(&mut s);
        (s.finish() % self.num_shards as u64) as usize
    }

    fn verify_auth<T>(&self, request: &Request<T>) -> Result<Claims, Status> {
        let metadata = request.metadata();
        let token = metadata
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| Status::unauthenticated("Missing or invalid Authorization header"))?;

        self.security
            .verify_token(token)
            .map_err(|e| Status::unauthenticated(format!("Invalid token: {}", e)))
    }
}

#[tonic::async_trait]
impl Agreda for AgredaService {
    type DownloadBlobStream = Pin<Box<dyn tokio_stream::Stream<Item = Result<BlobChunk, Status>> + Send + 'static>>;

    async fn login(&self, r: Request<LoginRequest>) -> Result<Response<AuthResponse>, Status> {
        let req = r.into_inner();
        match self.security.authenticate(&req.username, &req.password).await {
            Ok(token) => Ok(Response::new(AuthResponse {
                success: true,
                token,
                message: "Logged in".into(),
            })),
            Err(e) => Ok(Response::new(AuthResponse {
                success: false,
                token: String::new(),
                message: format!("Authentication failed: {}", e),
            })),
        }
    }

    async fn create_user(&self, r: Request<CreateUserRequest>) -> Result<Response<AuthResponse>, Status> {
        let req = r.into_inner();
        match self.security.create_user(&req.username, &req.password, &req.tenant).await {
            Ok(user) => {
                let token = match self.security.authenticate(&req.username, &req.password).await {
                    Ok(t) => t,
                    Err(e) => {
                        log::warn!("User '{}' created but token generation failed: {}", req.username, e);
                        String::new()
                    }
                };
                Ok(Response::new(AuthResponse {
                    success: true,
                    token,
                    message: format!("User '{}' created", user.username),
                }))
            }
            Err(e) => Ok(Response::new(AuthResponse {
                success: false,
                token: String::new(),
                message: format!("User creation failed: {}", e),
            })),
        }
    }

    async fn insert(&self, request: Request<InsertRequest>) -> Result<Response<InsertResponse>, Status> {
        let claims = self.verify_auth(&request)?;
        log::debug!("insert: user={} tenant={}", claims.username, claims.tenant);
        let req = request.into_inner();
        let shard_idx = self.get_shard(&req.id);
        let json_with_tenant = inject_tenant(&req.json_data, &claims.tenant);

        let (tx, rx) = flume::bounded(1);

        self.shards[shard_idx].send(InternalCommand::Insert {
            _table: req.table,
            json: json_with_tenant,
            responder: Some(tx)
        }).map_err(|_| Status::internal("Shard is dead"))?;

        match tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv_async()).await {
            Ok(Ok(_)) => Ok(Response::new(InsertResponse {
                success: true,
                message: "OK".into(),
                records_processed: 1
            })),
            Ok(Err(_)) => Err(Status::internal("Persistence channel closed")),
            Err(_) => Err(Status::deadline_exceeded("Request timeout")),
        }
    }

    async fn insert_batch(&self, request: Request<InsertBatchRequest>) -> Result<Response<InsertResponse>, Status> {
        let claims = self.verify_auth(&request)?;
        log::debug!("insert_batch: user={} tenant={}", claims.username, claims.tenant);
        let req = request.into_inner();
        let count = req.json_data_list.len() as u32;
        let shard_idx = self.get_shard(&req.table);
        let jsons_with_tenant: Vec<String> = req.json_data_list.iter()
            .map(|j| inject_tenant(j, &claims.tenant))
            .collect();

        let (tx, rx) = flume::bounded(1);
        self.shards[shard_idx].send(InternalCommand::InsertBatch {
            _table: req.table,
            jsons: jsons_with_tenant,
            responder: Some(tx)
        }).map_err(|_| Status::internal("Shard is dead"))?;

        rx.recv_async().await.map_err(|_| Status::internal("Persistence failed"))?;

        Ok(Response::new(InsertResponse { success: true, message: "Batch OK".into(), records_processed: count }))
    }

    async fn search(&self, request: Request<SearchRequest>) -> Result<Response<SearchResponse>, Status> {
        let claims = self.verify_auth(&request)?;
        log::debug!("search: user={} tenant={}", claims.username, claims.tenant);
        let req = request.into_inner();

        let mut receivers = Vec::with_capacity(self.num_shards);
        for sender in &self.shards {
            let (tx, rx) = flume::bounded(1);
            let _ = sender.send(InternalCommand::Search {
                _table: req.table.clone(),
                vector: req.vector.clone(),
                limit: req.limit,
                responder: tx,
            });
            receivers.push(rx);
        }

        let mut all_results = Vec::new();
        for rx in receivers {
            if let Ok(results) = rx.recv_async().await {
                all_results.extend(results);
            }
        }

        // Multi-tenant isolation: only return results belonging to the caller's tenant
        // Legacy data without _tenant is included (unwrap_or(true)) for backwards compatibility
        let tenant = claims.tenant.clone();
        all_results.retain(|r| {
            serde_json::from_str::<serde_json::Value>(&r.json_data)
                .ok()
                .and_then(|v| v.get("_tenant").and_then(|t| t.as_str()).map(|t| t == tenant))
                .unwrap_or(true)
        });

        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(req.limit as usize);

        Ok(Response::new(SearchResponse { results: all_results }))
    }

    async fn query(&self, request: Request<QueryRequest>) -> Result<Response<QueryResponse>, Status> {
        let claims = self.verify_auth(&request)?;
        log::debug!("query: user={} tenant={}", claims.username, claims.tenant);
        // NOTE: SQL queries should include WHERE _tenant = '<tenant>' for tenant isolation.
        // DataFusion does not enforce automatic tenant filtering yet.
        let req = request.into_inner();

        let batches = self.sql_engine
            .query(&req.sql)
            .await
            .map_err(|e| Status::internal(format!("SQL error: {}", e)))?;

        let mut buf = Vec::new();
        if !batches.is_empty() {
            let schema = batches[0].schema();
            let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut buf, &schema)
                .map_err(|e| Status::internal(format!("IPC serialization error: {}", e)))?;
            for batch in &batches {
                writer.write(batch)
                    .map_err(|e| Status::internal(format!("IPC write error: {}", e)))?;
            }
            writer.finish()
                .map_err(|e| Status::internal(format!("IPC finish error: {}", e)))?;
        }

        Ok(Response::new(QueryResponse { arrow_ipc_stream: buf }))
    }

    async fn upload_blob(&self, request: Request<Streaming<BlobChunk>>) -> Result<Response<BlobResponse>, Status> {
        let claims = self.verify_auth(&request)?;
        log::debug!("upload_blob: user={} tenant={}", claims.username, claims.tenant);

        let mut stream = request.into_inner();
        let blob_id = uuid::Uuid::new_v4().to_string();
        let file_path = format!("{}/{}.blob", self.blob_dir, blob_id);
        let mut all_data = Vec::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| Status::internal(format!("Stream error: {}", e)))?;
            all_data.extend_from_slice(&chunk.data);
        }

        tokio::fs::write(&file_path, &all_data).await
            .map_err(|e| Status::internal(format!("Failed to write blob: {}", e)))?;

        log::info!("Blob uploaded: id={} size={} bytes tenant={}", blob_id, all_data.len(), claims.tenant);
        Ok(Response::new(BlobResponse { success: true, blob_id }))
    }

    async fn download_blob(&self, request: Request<BlobRequest>) -> Result<Response<Self::DownloadBlobStream>, Status> {
        let claims = self.verify_auth(&request)?;
        log::debug!("download_blob: user={} tenant={}", claims.username, claims.tenant);

        let req = request.into_inner();
        // Sanitize blob_id to prevent path traversal attacks
        let safe_id: String = req.blob_id.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if safe_id.is_empty() {
            return Err(Status::invalid_argument("Invalid blob_id"));
        }
        let file_path = format!("{}/{}.blob", self.blob_dir, safe_id);

        let data = tokio::fs::read(&file_path).await
            .map_err(|_| Status::not_found(format!("Blob '{}' not found", safe_id)))?;

        let blob_id = safe_id;
        let chunks: Vec<Result<BlobChunk, Status>> = data.chunks(65536)
            .map(|chunk| Ok(BlobChunk { data: chunk.to_vec(), blob_id: blob_id.clone() }))
            .collect();

        let stream = tokio_stream::iter(chunks);
        Ok(Response::new(Box::pin(stream)))
    }
}

pub async fn start_server(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let service = AgredaService::new().await;

    println!("🚀 AgredaDB gRPC Server listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(AgredaServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
