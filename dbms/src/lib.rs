use datafusion::arrow::record_batch::RecordBatch;

pub mod sql_engine;
pub mod disk_ann;
pub mod blob_server;
pub mod sql_stream;

pub use sql_engine::{SqlEngine, AgredaTableProvider, AgredaExecutionPlan};
pub use disk_ann::DiskAnnIndex;
pub use blob_server::BlobEngine as BlobServer;

/// Comandos internos compartidos entre el Servidor y los Shards
pub enum InternalCommand {
    Insert { 
        _table: String, 
        json: String,
        responder: Option<flume::Sender<bool>>,
    },
    InsertBatch { 
        _table: String, 
        jsons: Vec<String>,
        responder: Option<flume::Sender<bool>>,
    },
    Query { 
        _table: String, 
        _col: Option<String>, 
        _val: Option<f64>, 
        responder: flume::Sender<RecordBatch>
    },
    Search { 
        _table: String, 
        vector: Vec<f32>, 
        limit: u32, 
        responder: flume::Sender<Vec<agreda_proto::SearchResult>> 
    },
}

pub mod agreda_proto {
    tonic::include_proto!("agreda");
}
