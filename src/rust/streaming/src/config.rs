use crate::{OrderedExecutor, Result};

/// Scheduling and file-output settings shared by streaming use cases.
pub struct ProcessingOptions {
    /// libdeflate compression level, validated for plain and gzip output alike.
    pub compression_level: i32,
    /// Records per work batch; zero means one record at a time.
    pub batch_size: usize,
    /// Output buffer target; a single record may exceed it.
    pub chunk_bytes: usize,
    /// Queued batches; `None` is unbounded, `Some(0)` allows no queued batches.
    pub nqueue: Option<usize>,
    /// Worker count, with a minimum of one.
    pub threads: usize,
}

impl ProcessingOptions {
    pub fn executor(&self) -> Result<OrderedExecutor> {
        OrderedExecutor::new(self.batch_size, self.nqueue, self.threads)
    }
}
