use std::error::Error;

use thiserror::Error;

/// Errors acquire operation and record context when they cross a port.
#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("{context}: {source}")]
    Operation {
        context: String,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0} worker panicked")]
    WorkerPanic(&'static str),
}

impl WorkflowError {
    pub fn operation(
        context: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::Operation {
            context: context.into(),
            source: Box::new(source),
        }
    }
}

pub type Result<T> = std::result::Result<T, WorkflowError>;

/// Streaming input, independent of file formats and scheduling.
pub trait RecordSource {
    type Record: Send;
    fn next_record(&mut self) -> Result<Option<Self::Record>>;
}

/// Ordered output, with explicit finalization so flush errors are observable.
pub trait RecordSink<T> {
    fn write_record(&mut self, record: T) -> Result<()>;
    fn finish(&mut self) -> Result<()>;
}

/// Executes a use case's transformation while preserving input order.
/// Scheduling, buffering, and worker lifetime belong to the adapter.
pub trait RecordExecutor {
    fn execute<S, W, F, T, U, E>(&self, source: S, sink: &mut W, transform: F) -> Result<()>
    where
        S: RecordSource<Record = T> + Send,
        W: RecordSink<U>,
        F: Fn(T) -> std::result::Result<Option<U>, E> + Sync,
        T: Send,
        U: Send,
        E: Error + Send + Sync + 'static;
}
