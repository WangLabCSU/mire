use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

#[cfg(not(feature = "isal"))]
use flate2::bufread::GzDecoder;
use indicatif::{ProgressBar, ProgressFinish, ProgressStyle};
#[cfg(feature = "isal")]
use isal::read::GzipDecoder;
use libdeflater::{CompressionLvl, Compressor};

use super::reader::{ProgressBarReader, ProgressBarWriter};
use crate::ports::{Result, WorkflowError};

pub const BUFFER_SIZE: usize = 4 * 1024 * 1024;

fn is_gzip(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
}

pub fn open_input(path: &Path) -> Result<Box<dyn Read + Send>> {
    let file = File::open(path)
        .map_err(|error| WorkflowError::operation(format!("open '{}'", path.display()), error))?;
    let length = file
        .metadata()
        .map_err(|error| WorkflowError::operation("read input metadata", error))?
        .len();
    let bar = progress("Reading", Some(length));
    let reader = ProgressBarReader::new(file, bar);
    if !is_gzip(path) {
        return Ok(Box::new(reader));
    }
    let reader = BufReader::with_capacity(BUFFER_SIZE, reader);
    #[cfg(not(feature = "isal"))]
    let reader = GzDecoder::new(reader);
    #[cfg(feature = "isal")]
    let reader = GzipDecoder::new(reader);
    Ok(Box::new(reader))
}

fn progress(prefix: &'static str, length: Option<u64>) -> ProgressBar {
    let bar = length
        .map_or_else(ProgressBar::no_length, ProgressBar::new)
        .with_finish(ProgressFinish::Abandon);
    bar.set_prefix(prefix);
    bar.set_style(ProgressStyle::default_bar());
    bar
}

/// Output buffering and compression are independent of biological records.
/// Holds at most one configured chunk plus the largest serialized record.
pub struct ChunkWriter {
    writer: Box<dyn Write>,
    buffer: Vec<u8>,
    chunk_bytes: usize,
    compressor: Option<Compressor>,
    context: String,
}

impl ChunkWriter {
    pub fn open(path: &Path, chunk_bytes: usize, compression_level: i32) -> Result<Self> {
        let level = CompressionLvl::new(compression_level).map_err(|error| {
            WorkflowError::InvalidRequest(format!("Invalid 'compression_level': {error:?}"))
        })?;
        let context = format!("write '{}'", path.display());
        let file = File::create(path).map_err(|error| WorkflowError::operation(&context, error))?;
        Ok(Self {
            writer: Box::new(BufWriter::new(ProgressBarWriter::new(
                file,
                progress("Writing", None),
            ))),
            buffer: Vec::with_capacity(chunk_bytes),
            chunk_bytes,
            compressor: is_gzip(path).then(|| Compressor::new(level)),
            context,
        })
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        if !self.buffer.is_empty()
            && self.buffer.len().saturating_add(bytes.len()) > self.chunk_bytes
        {
            self.flush_chunk()?;
        }
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    fn flush_chunk(&mut self) -> Result<()> {
        if let Some(compressor) = &mut self.compressor {
            let mut compressed = vec![0; compressor.gzip_compress_bound(self.buffer.len())];
            let length = compressor
                .gzip_compress(&self.buffer, &mut compressed)
                .map_err(|error| WorkflowError::operation("compress output chunk", error))?;
            self.writer
                .write_all(&compressed[..length])
                .map_err(|error| WorkflowError::operation(&self.context, error))?;
        } else {
            self.writer
                .write_all(&self.buffer)
                .map_err(|error| WorkflowError::operation(&self.context, error))?;
        }
        self.buffer.clear();
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        if !self.buffer.is_empty() {
            self.flush_chunk()?;
        }
        self.writer
            .flush()
            .map_err(|error| WorkflowError::operation(&self.context, error))
    }
}
