use thiserror::Error;

/// Failures loading taxonomy or processing Kraken classifications and reads.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Workflow(#[from] mire_streaming::WorkflowError),
}

pub type Result<T> = std::result::Result<T, Error>;
