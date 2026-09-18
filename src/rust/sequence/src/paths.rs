use crate::io::records::{FastqSink, FastqSource};
use mire_streaming::{ProcessingOptions, Result, WorkflowError};
use std::path::Path;

/// Input and output paths for a single read stream or a synchronized read pair.
pub struct FastqPaths<'a> {
    pub input1: &'a str,
    pub input2: Option<&'a str>,
    pub output1: Option<&'a str>,
    pub output2: Option<&'a str>,
}

impl FastqPaths<'_> {
    pub(crate) fn source(&self) -> Result<FastqSource> {
        FastqSource::open(Path::new(self.input1), self.input2.map(Path::new))
    }
    pub(crate) fn sink(&self, options: &ProcessingOptions) -> Result<FastqSink> {
        if self.input2.is_none() && self.output1.is_none() {
            return Err(WorkflowError::InvalidRequest(
                "No output file specified.".into(),
            ));
        }
        FastqSink::open(
            self.output1.map(Path::new),
            self.input2.and(self.output2).map(Path::new),
            options.chunk_bytes,
            options.compression_level,
        )
    }
}
