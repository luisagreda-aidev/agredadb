use std::pin::Pin;
use std::task::{Context, Poll};
use futures::{Stream, Future};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::error::Result;
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::physical_plan::RecordBatchStream;

/// AgredaDB RecordBatch Stream Real
/// Implementación que utiliza el receptor asíncrono de Flume para evitar Busy-Waiting.
pub struct AgredaRecordBatchStream {
    pub schema: SchemaRef,
    pub receiver: flume::Receiver<RecordBatch>,
    pub current_future: Option<Pin<Box<dyn Future<Output = std::result::Result<RecordBatch, flume::RecvError>> + Send>>>,
}

impl Stream for AgredaRecordBatchStream {
    type Item = Result<RecordBatch>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut fut = match self.current_future.take() {
            Some(fut) => fut,
            None => Box::pin(self.receiver.clone().into_recv_async()),
        };

        match fut.as_mut().poll(cx) {
            Poll::Ready(Ok(batch)) => Poll::Ready(Some(Ok(batch))),
            Poll::Ready(Err(_)) => Poll::Ready(None), // Canal cerrado
            Poll::Pending => {
                self.current_future = Some(fut);
                Poll::Pending
            }
        }
    }
}

impl RecordBatchStream for AgredaRecordBatchStream {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}