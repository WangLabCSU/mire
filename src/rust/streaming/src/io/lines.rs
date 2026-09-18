use super::{open_input, ChunkWriter, LineReader, BUFFER_SIZE};
use crate::{RecordSink, RecordSource, Result, WorkflowError};
use bytes::Bytes;
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub struct LineSource {
    reader: LineReader<Box<dyn Read + Send>>,
    path: PathBuf,
}

impl LineSource {
    pub fn open_plain(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|error| {
            WorkflowError::operation(format!("open '{}'", path.display()), error)
        })?;
        Ok(Self {
            reader: LineReader::with_capacity(BUFFER_SIZE, Box::new(file)),
            path: path.to_owned(),
        })
    }

    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            reader: LineReader::with_capacity(BUFFER_SIZE, open_input(path)?),
            path: path.to_owned(),
        })
    }
}

impl RecordSource for LineSource {
    type Record = Bytes;

    fn next_record(&mut self) -> Result<Option<Bytes>> {
        self.reader.read_line().map_err(|error| {
            WorkflowError::operation(
                format!(
                    "read '{}' at line {}",
                    self.path.display(),
                    self.reader.offset() + 1
                ),
                error,
            )
        })
    }
}

impl RecordSink<Bytes> for ChunkWriter {
    fn write_record(&mut self, line: Bytes) -> Result<()> {
        self.write_bytes(&line)?;
        self.write_bytes(b"\n")
    }

    fn finish(&mut self) -> Result<()> {
        self.finish()
    }
}
