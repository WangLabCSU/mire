use std::error::Error;

use thiserror::Error;

/// A streaming operation, request or worker failed.
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

/// Read records sequentially from an input source.
pub trait RecordSource {
    type Record: Send;
    fn next_record(&mut self) -> Result<Option<Self::Record>>;
}

/// Ordered output, with explicit finalization so flush errors are observable.
pub trait RecordSink<T> {
    fn write_record(&mut self, record: T) -> Result<()>;
    fn finish(&mut self) -> Result<()>;
}

/// Transform input records and write retained results in input order.
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
