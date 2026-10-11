use std::io::{self, BufRead, ErrorKind, Read};

use bytes::{Buf, BytesMut};
use memchr::memchr;

use crate::DelimPolicy;

const DEFAULT_BUFFER_SIZE: usize = 8 * 1024;

/// A buffered reader that returns owned byte segments.
///
/// `BytesReader<R>` reads from a [`Read`] source and returns [`BytesMut`] values
/// for lines and delimited segments. These values can be held and modified
/// independently of the reader. No UTF-8 validation or text conversion occurs.
///
/// [`Self::read_line`], [`Self::read_delim`], [`Read`], and [`BufRead`] use the
/// same input position. Dropping the reader discards buffered bytes that have
/// not been returned to the caller. Previously returned values remain valid.
///
/// Lines have no fixed length limit. Holding results can retain more memory than
/// their visible lengths. Allocation failure follows the standard allocator's
/// behavior and is not guaranteed to return a recoverable I/O error.
///
/// # Examples
///
/// ```
/// use std::io;
///
/// use bytes::BytesMut;
/// use mire_byteio::{BytesReader, DelimPolicy};
///
/// let mut reader = BytesReader::with_capacity(4, b"first\r\nsecond\n".as_slice());
/// let first: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
/// let second: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
/// drop(reader);
/// assert_eq!(first.as_ref(), b"first");
/// assert_eq!(second.as_ref(), b"second");
/// # Ok::<(), io::Error>(())
/// ```
pub struct BytesReader<R> {
    inner: R,
    // Unread input. Consuming bytes advances this view into the allocation.
    buffer: BytesMut,
    // Requested length of each refill, independent of the current capacity.
    buffer_size: usize,
}

impl<R: Read> BytesReader<R> {
    /// Creates a byte reader using the default buffer size.
    ///
    /// The current default is 8 KiB; it may change in a future release.
    ///
    /// # Examples
    ///
    /// ```
    /// use mire_byteio::BytesReader;
    ///
    /// let reader = BytesReader::new(b"first\nsecond\n".as_slice());
    /// ```
    pub fn new(source: R) -> Self {
        Self::with_capacity(DEFAULT_BUFFER_SIZE, source)
    }

    /// Creates a byte reader with the specified input buffer size.
    ///
    /// Each refill requests up to `buffer_size` bytes. This size remains fixed
    /// and does not limit the length of a line or delimited segment.
    ///
    /// A zero size is allowed. In that case, successful [`BufRead::fill_buf`]
    /// calls return an empty slice, and [`Self::read_line`] and
    /// [`Self::read_delim`] return `None` even if the source has more data.
    /// [`Read::read`] can still read directly from the source.
    ///
    /// # Panics
    ///
    /// Panics if `buffer_size` exceeds the allocation's capacity limit.
    ///
    /// # Examples
    ///
    /// ```
    /// use mire_byteio::BytesReader;
    ///
    /// let reader = BytesReader::with_capacity(1024, b"first\nsecond\n".as_slice());
    /// assert!(reader.capacity() >= 1024);
    /// ```
    pub fn with_capacity(buffer_size: usize, inner: R) -> Self {
        Self {
            inner,
            buffer: BytesMut::with_capacity(buffer_size),
            buffer_size,
        }
    }

    /// Returns the capacity of the current input buffer, in bytes.
    ///
    /// The capacity can change after reading and may differ from the configured
    /// buffer size. It does not limit line lengths or include memory held by
    /// previously returned results.
    ///
    /// # Examples
    ///
    /// ```
    /// use mire_byteio::BytesReader;
    ///
    /// let reader = BytesReader::with_capacity(4, b"input".as_slice());
    /// assert!(reader.capacity() >= 4);
    /// ```
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Reads a byte segment through the first occurrence of `delim` or EOF.
    ///
    /// [`DelimPolicy::Include`] keeps the delimiter in the result.
    /// [`DelimPolicy::Exclude`] removes only the delimiter, preserving any
    /// preceding CR. Both policies consume the delimiter when it is found.
    ///
    /// Returns `Some` for a segment, including an empty segment, or `None` when
    /// no bytes are available. A final segment without a delimiter is returned
    /// unchanged. See [`Self::with_capacity`] for the behavior of a zero size.
    ///
    /// # Errors
    ///
    /// Retries [`ErrorKind::Interrupted`] and returns other I/O errors.
    /// Bytes read before an error remain in the reader and can be recovered by
    /// a subsequent read, including one using a different method or delimiter.
    ///
    /// # Examples
    ///
    /// ```
    /// use mire_byteio::{BytesReader, DelimPolicy};
    ///
    /// let mut reader = BytesReader::new(b"\xff|tail".as_slice());
    /// let first = reader.read_delim(b'|', DelimPolicy::Include)?.unwrap();
    /// assert_eq!(first.as_ref(), b"\xff|");
    /// let last = reader.read_delim(b'|', DelimPolicy::Exclude)?.unwrap();
    /// assert_eq!(last.as_ref(), b"tail");
    /// assert!(reader.read_delim(b'|', DelimPolicy::Exclude)?.is_none());
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn read_delim(
        &mut self,
        delim: u8,
        delim_policy: DelimPolicy,
    ) -> io::Result<Option<BytesMut>> {
        self.read_delimited(delim, delim_policy, false)
    }

    /// Reads a byte line through the next LF (`0x0A`) or EOF.
    ///
    /// [`DelimPolicy::Include`] keeps the line ending.
    /// [`DelimPolicy::Exclude`] removes LF and one immediately preceding CR.
    /// Isolated CR and all other bytes are preserved.
    ///
    /// Returns `Some` for a line, including an empty line, or `None` when no
    /// bytes are available. A final line without LF is returned unchanged; a
    /// trailing LF does not produce an extra empty line. See
    /// [`Self::with_capacity`] for the behavior of a zero size.
    ///
    /// # Errors
    ///
    /// Follows [`Self::read_delim`]: interrupted reads are retried, and other
    /// I/O errors leave undelivered bytes available for a subsequent read.
    ///
    /// # Examples
    ///
    /// ```
    /// use mire_byteio::{BytesReader, DelimPolicy};
    ///
    /// let mut reader = BytesReader::new(b"\xff\r\n\nlast\r".as_slice());
    /// let first = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    /// assert_eq!(first.as_ref(), b"\xff");
    /// assert!(reader.read_line(DelimPolicy::Exclude)?.unwrap().is_empty());
    /// let last = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    /// assert_eq!(last.as_ref(), b"last\r");
    /// assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn read_line(&mut self, delim_policy: DelimPolicy) -> io::Result<Option<BytesMut>> {
        self.read_delimited(b'\n', delim_policy, true)
    }

    fn read_delimited(
        &mut self,
        delim: u8,
        delim_policy: DelimPolicy,
        strip_cr: bool,
    ) -> io::Result<Option<BytesMut>> {
        // None means no prefix has been consumed. Some holds a nonempty
        // prefix with no delimiter, so only the current buffer needs searching.
        let mut out: Option<BytesMut> = None;
        loop {
            let available = match self.fill_buf() {
                Ok(a) => a,
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(err) => {
                    // `fill_buf` leaves the buffer empty on error. Restore any
                    // consumed prefix for subsequent reading methods to recover.
                    if let Some(leftover) = out {
                        self.buffer = leftover;
                    }
                    return Err(err);
                }
            };
            if available.is_empty() {
                // Preserve an unterminated prefix; no prefix means no result.
                return Ok(out);
            }

            if let Some(pos) = memchr(delim, available) {
                let mut delimited = match out {
                    Some(mut leftover) => {
                        // Append through the delimiter, leaving the tail
                        // buffered for the next reading operation.
                        leftover.extend_from_slice(&available[..pos + 1]);
                        self.consume(pos + 1);
                        leftover
                    }
                    // Splitting shares the allocation without copying bytes,
                    // but may allocate metadata to track the shared storage.
                    None => self.buffer.split_to(pos + 1),
                };
                // Apply the policy to the complete segment, since CRLF may
                // span buffers. An empty result after truncation is still Some.
                let end = delim_policy.delim_end(&delimited, strip_cr);
                delimited.truncate(end);
                return Ok(Some(delimited));
            }

            if let Some(out) = out.as_mut() {
                // Append subsequent blocks, growing the result if needed.
                out.extend_from_slice(available);
            } else {
                // Copy the first block, leaving the input allocation
                // available for subsequent refills.
                out = Some(self.buffer.clone());
            }
            // Consume this block so the next iteration can refill it.
            self.consume(self.buffer.len());
        }
    }
}

impl<R: Read> BufRead for BytesReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        // If we've reached the end of our internal buffer then we need to fetch
        // some more data from the reader.
        if self.buffer.is_empty() {
            // `Read::read` requires initialized bytes, so zero the destination.
            // If this view is too small, BytesMut may reclaim space or allocate.
            // Storage still held by split results cannot be reclaimed here.
            self.buffer.resize(self.buffer_size, 0);
            match self.inner.read(&mut self.buffer) {
                // A short read must not expose the unused zero-filled tail.
                Ok(n) => self.buffer.truncate(n),
                Err(err) => {
                    // Do not expose the resized destination as buffered input
                    // after a failed read.
                    self.buffer.clear();
                    return Err(err);
                }
            }
        }
        Ok(&self.buffer)
    }
    fn consume(&mut self, amount: usize) {
        self.buffer.advance(amount);
    }
}

impl<R: Read> Read for BytesReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If there is no buffered input and the read is large enough, bypass
        // the buffer to avoid an extra copy. Use the configured size because
        // splitting off results can reduce the buffer's current capacity.
        if self.buffer.is_empty() && buf.len() >= self.buffer_size {
            return self.inner.read(buf);
        }
        let mut rem = self.fill_buf()?;
        let nread = rem.read(buf)?;
        self.consume(nread);
        Ok(nread)
    }
}
