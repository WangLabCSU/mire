//! Shared streaming ports and file/execution adapters.
//!
//! Read and write records, handle plain or gzip files, and execute transformations
//! while preserving record order.
mod config;
mod execution;
mod io;
mod ports;

pub use config::ProcessingOptions;
pub use execution::OrderedExecutor;
pub use io::{open_input, ChunkWriter, LineReader, LineSource, BUFFER_SIZE};
pub use ports::{RecordExecutor, RecordSink, RecordSource, Result, WorkflowError};
