use bytes::Bytes;
use thiserror::Error;

use super::record::FastqRecord;

/// An owned FASTQ record backed by shared byte storage.
pub type Read = FastqRecord<Bytes>;

/// The sequencing unit processed by a use case, preserving read-end order.
#[derive(Debug)]
pub struct ReadFragment {
    pub(crate) read1: Read,
    pub(crate) read2: Option<Read>,
}

#[derive(Debug, Error)]
pub(crate) enum ReadPairError {
    #[error("FASTQ pairing error: sequence IDs do not match ('{read1}' and '{read2}')")]
    DifferentIds { read1: String, read2: String },
    #[error("FASTQ pairing error: record count mismatch at read pair {pair}")]
    DifferentCounts { pair: usize },
}

impl ReadFragment {
    pub(crate) fn single(read1: Read) -> Self {
        Self { read1, read2: None }
    }

    /// Constructs a read pair, rejecting different sequence IDs.
    pub(crate) fn paired(read1: Read, read2: Read) -> Result<Self, ReadPairError> {
        if read1.id != read2.id {
            return Err(ReadPairError::DifferentIds {
                read1: String::from_utf8_lossy(&read1.id).into_owned(),
                read2: String::from_utf8_lossy(&read2.id).into_owned(),
            });
        }
        Ok(Self {
            read1,
            read2: Some(read2),
        })
    }

    /// The ID shared by the read ends.
    pub fn id(&self) -> &[u8] {
        &self.read1.id
    }

    /// The first (or only) read end.
    pub fn first(&self) -> &Read {
        &self.read1
    }

    /// The second read end, when paired.
    pub fn second(&self) -> Option<&Read> {
        self.read2.as_ref()
    }

    /// Consume the fragment, preserving first/second read order.
    pub fn into_reads(self) -> (Read, Option<Read>) {
        (self.read1, self.read2)
    }
}
