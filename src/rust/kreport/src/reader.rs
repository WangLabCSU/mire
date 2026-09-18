//! Private report file reader. Buffering semantics are preserved from the existing reader.
use bytes::{Bytes, BytesMut};
use memchr::memchr;
use std::io::Read;

/// LineReader: Efficient zero-copy line-based reader using BytesMut.
///
/// This reader avoids unnecessary heap allocations and copying by:
/// - Reusing a fixed-size buffer (`BytesMut`)
/// - Using `split_to()` to transfer ownership without copying
/// - Accumulating "leftover" when a line spans multiple reads
///
/// Supports CRLF or LF endings and returns each line as `Bytes`.
pub(crate) struct LineReader<R> {
    reader: R,                  // Underlying reader (e.g., File)
    offset: usize,              // Line count
    buffer_size: usize,         // buffer capacity
    buffer: Option<BytesMut>,   // Current buffer filled from reader
    leftover: Option<BytesMut>, // Accumulates data when line spans multiple buffers
}

impl<R: Read> LineReader<R> {
    #[inline]
    pub(crate) fn with_capacity(capacity: usize, reader: R) -> Self {
        Self {
            reader,
            offset: 0,
            buffer: None,
            buffer_size: capacity,
            leftover: None,
        }
    }

    #[inline]
    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    #[inline]
    pub(crate) fn read_line(&mut self) -> std::io::Result<Option<Bytes>> {
        loop {
            self.fill_buf()?;
            if let Some(buffer) = self.buffer.as_mut() {
                if let Some(pos) = memchr(b'\n', buffer) {
                    // Fast path: newline found
                    let mut buf = buffer.split_to(pos + 1);
                    let end = if pos > 0 && buf[pos - 1] == b'\r' {
                        pos - 1
                    } else {
                        pos
                    };
                    let line = if let Some(mut leftover) = self.leftover.take() {
                        leftover.extend_from_slice(&buf[..end]);
                        leftover
                    } else {
                        // Directly build from slice without heap copying if possible
                        buf.split_to(end)
                    };
                    self.offset += 1;
                    return Ok(Some(line.freeze()));
                }

                // No newline: accumulate leftover and continue
                if let Some(left) = self.leftover.as_mut() {
                    left.extend_from_slice(buffer);
                    self.buffer = None
                } else {
                    std::mem::swap(&mut self.buffer, &mut self.leftover);
                }
            } else {
                let left = std::mem::take(&mut self.leftover).map(|l| l.freeze());
                if left.is_some() {
                    self.offset += 1;
                }
                return Ok(left);
            }
        }
    }

    #[inline]
    fn fill_buf(&mut self) -> std::io::Result<()> {
        if self.buffer.is_none() {
            let mut buffer = BytesMut::zeroed(self.buffer_size);
            let nbytes = self.reader.read(&mut buffer)?;
            buffer.truncate(nbytes);
            if nbytes > 0 {
                self.buffer = Some(buffer)
            }
        }
        Ok(())
    }
}
