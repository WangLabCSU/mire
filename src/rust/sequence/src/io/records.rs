use super::fastq_read::FastqReader;
use crate::{ReadFragment, ReadPairError};
use mire_streaming::{
    open_input, ChunkWriter, RecordSink, RecordSource, Result, WorkflowError, BUFFER_SIZE,
};
use std::{io::Read, path::Path};

pub(crate) struct FastqSource {
    first: FastqReader<Box<dyn Read + Send>>,
    second: Option<FastqReader<Box<dyn Read + Send>>>,
    paths: String,
    count: usize,
}

impl FastqSource {
    pub(crate) fn open(read1: &Path, read2: Option<&Path>) -> Result<Self> {
        Ok(Self {
            first: FastqReader::with_capacity(BUFFER_SIZE, open_input(read1)?),
            second: read2
                .map(|path| {
                    open_input(path).map(|reader| FastqReader::with_capacity(BUFFER_SIZE, reader))
                })
                .transpose()?,
            paths: format!(
                "FASTQ '{}'{}",
                read1.display(),
                read2.map_or_else(String::new, |path| format!(" and '{}'", path.display()))
            ),
            count: 0,
        })
    }
}

impl RecordSource for FastqSource {
    type Record = ReadFragment;

    fn next_record(&mut self) -> Result<Option<ReadFragment>> {
        let first = self
            .first
            .read_record()
            .map_err(|error| WorkflowError::Operation {
                context: self.paths.clone(),
                source: error.into(),
            })?;
        let Some(reader2) = &mut self.second else {
            return Ok(first.map(ReadFragment::single));
        };
        let second = reader2
            .read_record()
            .map_err(|error| WorkflowError::Operation {
                context: self.paths.clone(),
                source: error.into(),
            })?;
        self.count += 1;
        match (first, second) {
            (Some(first), Some(second)) => ReadFragment::paired(first, second)
                .map(Some)
                .map_err(|error| WorkflowError::operation(&self.paths, error)),
            (None, None) => Ok(None),
            _ => Err(WorkflowError::operation(
                &self.paths,
                ReadPairError::DifferentCounts { pair: self.count },
            )),
        }
    }
}

pub(crate) struct FastqSink {
    first: Option<ChunkWriter>,
    second: Option<ChunkWriter>,
    buffer: Vec<u8>,
}

impl FastqSink {
    pub(crate) fn open(
        first: Option<&Path>,
        second: Option<&Path>,
        chunk_bytes: usize,
        compression: i32,
    ) -> Result<Self> {
        if first.is_none() && second.is_none() {
            return Err(WorkflowError::InvalidRequest(
                "No output file specified.".into(),
            ));
        }
        Ok(Self {
            first: first
                .map(|path| ChunkWriter::open(path, chunk_bytes, compression))
                .transpose()?,
            second: second
                .map(|path| ChunkWriter::open(path, chunk_bytes, compression))
                .transpose()?,
            buffer: Vec::new(),
        })
    }
}

impl RecordSink<ReadFragment> for FastqSink {
    fn write_record(&mut self, record: ReadFragment) -> Result<()> {
        if let Some(writer) = &mut self.first {
            self.buffer.clear();
            record.read1.extend(&mut self.buffer);
            writer.write_bytes(&self.buffer)?;
        }
        if let (Some(writer), Some(read)) = (&mut self.second, record.read2) {
            self.buffer.clear();
            read.extend(&mut self.buffer);
            writer.write_bytes(&self.buffer)?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        if let Some(writer) = &mut self.first {
            writer.finish()?;
        }
        if let Some(writer) = &mut self.second {
            writer.finish()?;
        }
        Ok(())
    }
}
