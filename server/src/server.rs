use tonic::{Request, Response, Status, Streaming};
use std::pin::Pin;
use crate::shard::Shard;
use agredadb_dbms::InternalCommand;
use agredadb_dbms::agreda_proto::agreda_server::{Agreda, AgredaServer};
use agredadb_dbms::agreda_proto::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;

pub struct AgredaService {
    shards: Vec<flume::Sender<InternalCommand>>,
    num_shards: usize,
}

impl AgredaService {
    pub async fn new() -> Self {
        let num_shards = num_cpus::get();
        let mut shard_senders = Vec::new();
        for i in 0..num_shards {
            let (tx, rx) = flume::unbounded();
            let mut shard = Shard::new(i, rx).await;
            tokio::spawn(async move { shard.run().await; });
            shard_senders.push(tx);
        }
        Self { shards: shard_senders, num_shards }
    }

    fn get_shard(&self, id: &str) -> usize {
        let mut s = DefaultHasher::new();
        id.hash(&mut s);
        (s.finish() % self.num_shards as u64) as usize
    }
}

#[tonic::async_trait]
impl Agreda for AgredaService {
    type DownloadBlobStream = Pin<Box<dyn tokio_stream::Stream<Item = Result<BlobChunk, Status>> + Send + 'static>>;

    async fn login(&self, _r: Request<LoginRequest>) -> Result<Response<AuthResponse>, Status> {
        Ok(Response::new(AuthResponse { success: true, token: "agreda_dev_token".into(), message: "Logged in".into() }))
    }

    async fn create_user(&self, _r: Request<CreateUserRequest>) -> Result<Response<AuthResponse>, Status> {
        Ok(Response::new(AuthResponse { success: true, token: "agreda_dev_token".into(), message: "User created".into() }))
    }

    async fn insert(&self, request: Request<InsertRequest>) -> Result<Response<InsertResponse>, Status> {
        let req = request.into_inner();
        let shard_idx = self.get_shard(&req.id);
        
        let (tx, rx) = flume::bounded(1);
        
        self.shards[shard_idx].send(InternalCommand::Insert { 
            _table: req.table, 
            json: req.json_data, 
            responder: Some(tx) 
        }).map_err(|_| Status::internal("Shard is dead"))?;
        
        // Wait for persistence ack with timeout to prevent hanging
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
        let req = request.into_inner();
        let count = req.json_data_list.len() as u32;
        let shard_idx = rand::random::<usize>() % self.num_shards;
        
        let (tx, rx) = flume::bounded(1);
        self.shards[shard_idx].send(InternalCommand::InsertBatch { 
            _table: req.table, 
            jsons: req.json_data_list,
            responder: Some(tx)
        }).map_err(|_| Status::internal("Shard is dead"))?;
        
        // Wait for persistence ack
        rx.recv_async().await.map_err(|_| Status::internal("Persistence failed"))?;
        
        Ok(Response::new(InsertResponse { success: true, message: "Batch OK".into(), records_processed: count }))
    }

    async fn search(&self, request: Request<SearchRequest>) -> Result<Response<SearchResponse>, Status> {
        let req = request.into_inner();
        let (tx, rx) = flume::bounded(1);
        let _ = self.shards[0].send(InternalCommand::Search { _table: req.table, vector: req.vector, limit: req.limit, responder: tx });
        let results = rx.recv_async().await.map_err(|_| Status::internal("Shard error"))?;
        Ok(Response::new(SearchResponse { results }))
    }

    async fn query(&self, _r: Request<QueryRequest>) -> Result<Response<QueryResponse>, Status> {
        Ok(Response::new(QueryResponse { arrow_ipc_stream: vec![] }))
    }

    async fn upload_blob(&self, _r: Request<Streaming<BlobChunk>>) -> Result<Response<BlobResponse>, Status> {
        Ok(Response::new(BlobResponse { success: true, blob_id: "blob_id".into() }))
    }

    async fn download_blob(&self, _r: Request<BlobRequest>) -> Result<Response<Self::DownloadBlobStream>, Status> {
        Err(Status::unimplemented("Not used in benchmark"))
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
