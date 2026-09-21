use thiserror::Error;

/// Failures loading taxonomy or counting classified reads and k-mers.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Workflow(#[from] mire_streaming::WorkflowError),
}

pub type Result<T> = std::result::Result<T, Error>;
