//! Shared streaming ports and file/execution adapters.
//!
//! This crate has no dependency on sequencing, classification, reports, or R.
mod config;
mod execution;
mod io;
mod ports;

pub use config::ProcessingOptions;
pub use execution::OrderedExecutor;
pub use io::{open_input, ChunkWriter, LineReader, LineSource, BUFFER_SIZE};
pub use ports::{RecordExecutor, RecordSink, RecordSource, Result, WorkflowError};
