use std::sync::Arc;
use datafusion::error::Result;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::TableProvider;
use datafusion::execution::context::{SessionContext};
use async_trait::async_trait;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::physical_plan::{ExecutionPlan, DisplayAs, DisplayFormatType, SendableRecordBatchStream, PlanProperties, Partitioning};
use datafusion::logical_expr::{TableType, Expr};
use datafusion::physical_expr::EquivalenceProperties;
use datafusion::execution::TaskContext;
use std::any::Any;

use crate::sql_stream::AgredaRecordBatchStream;
use crate::InternalCommand;

pub struct SqlEngine {
    ctx: SessionContext,
}

impl SqlEngine {
    pub fn new() -> Self {
        let ctx = SessionContext::new();
        Self { ctx }
    }

    pub fn register_native_table(&self, name: &str, provider: Arc<dyn TableProvider>) -> Result<()> {
        self.ctx.register_table(name, provider)?;
        Ok(())
    }

    pub async fn query(&self, sql: &str) -> Result<Vec<RecordBatch>> {
        let df = self.ctx.sql(sql).await?;
        df.collect().await
    }
}

pub struct AgredaTableProvider {
    schema: SchemaRef,
    shard_channels: Vec<flume::Sender<InternalCommand>>,
}

impl AgredaTableProvider {
    pub fn new(schema: SchemaRef, channels: Vec<flume::Sender<InternalCommand>>) -> Self {
        Self { schema, shard_channels: channels }
    }
}

#[async_trait]
impl TableProvider for AgredaTableProvider {
    fn as_any(&self) -> &dyn Any { self }
    fn schema(&self) -> SchemaRef { self.schema.clone() }
    fn table_type(&self) -> TableType { TableType::Base }

    async fn scan(
        &self,
        _state: &dyn datafusion::catalog::Session,
        _projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        Ok(Arc::new(AgredaExecutionPlan::new(self.schema.clone(), self.shard_channels.clone())))
    }
}

#[derive(Debug)]
pub struct AgredaExecutionPlan {
    schema: SchemaRef,
    shard_channels: Vec<flume::Sender<InternalCommand>>,
    properties: PlanProperties,
}

impl AgredaExecutionPlan {
    pub fn new(schema: SchemaRef, channels: Vec<flume::Sender<InternalCommand>>) -> Self {
        let properties = PlanProperties::new(
            EquivalenceProperties::new(schema.clone()),
            Partitioning::UnknownPartitioning(1),
            datafusion::physical_plan::ExecutionMode::Bounded,
        );
        Self { schema, shard_channels: channels, properties }
    }
}

impl DisplayAs for AgredaExecutionPlan {
    fn fmt_as(&self, _t: DisplayFormatType, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "AgredaExecutionPlan")
    }
}

impl ExecutionPlan for AgredaExecutionPlan {
    fn as_any(&self) -> &dyn Any { self }
    fn schema(&self) -> SchemaRef { self.schema.clone() }
    
    fn properties(&self) -> &PlanProperties {
        &self.properties
    }

    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> { vec![] }
    
    fn with_new_children(self: Arc<Self>, _children: Vec<Arc<dyn ExecutionPlan>>) -> Result<Arc<dyn ExecutionPlan>> {
        Ok(self)
    }
    
    fn execute(&self, _partition: usize, _context: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
        let (tx, rx) = flume::unbounded();
        
        if self.shard_channels.is_empty() {
            drop(tx);
        } else {
            for channel in &self.shard_channels {
                let _ = channel.send(InternalCommand::Query {
                    _table: "native_scan".to_string(),
                    _col: None,
                    _val: None,
                    responder: tx.clone(),
                });
            }
        }
        
        Ok(Box::pin(AgredaRecordBatchStream {
            schema: self.schema.clone(),
            receiver: rx,
            current_future: None,
        }))
    }

    fn name(&self) -> &str {
        "AgredaExecutionPlan"
    }
}