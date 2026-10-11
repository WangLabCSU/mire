use std::io::{self, Write};

use bytes::{Buf, BytesMut};

use crate::Encoder;

const INITIAL_CAPACITY: usize = 8 * 1024;

/// A buffered writer of frames encoded to a [`Write`] destination.
///
/// Call [`flush`](Self::flush) to write pending frames. Dropping this adapter
/// discards buffered output without flushing it.
///
/// # Examples
///
/// ```
/// use std::io;
///
/// use bytes::BytesMut;
/// use codec::{Encoder, FramedWrite};
///
/// struct Pairs;
///
/// impl Encoder<[u8; 2]> for Pairs {
///     type Error = io::Error;
///
///     fn encode(&mut self, item: [u8; 2], dst: &mut BytesMut) -> io::Result<()> {
///         dst.extend_from_slice(&item);
///         Ok(())
///     }
/// }
///
/// let mut writer = FramedWrite::new(Vec::new(), Pairs);
/// writer.write([0xff, 0])?;
/// writer.flush()?;
/// assert_eq!(writer.into_inner(), [0xff, 0]);
/// # Ok::<(), io::Error>(())
/// ```
#[derive(Debug)]
pub struct FramedWrite<W, E> {
    inner: W,
    encoder: E,
    // Contains only bytes not yet accepted by the underlying writer. Advance
    // after each successful write so a later error leaves exactly its suffix.
    buffer: BytesMut,
    // Checked before encoding the next frame. This is independent of allocation
    // capacity: one large frame may grow the buffer without raising the threshold.
    backpressure_boundary: usize,
}

impl<W, E> FramedWrite<W, E> {
    /// Creates a writer using `encoder` and the default buffer capacity.
    ///
    /// The default is currently 8 KiB and may change in the future.
    pub fn new(inner: W, encoder: E) -> Self {
        Self::with_capacity(inner, encoder, INITIAL_CAPACITY)
    }

    /// Creates a writer with at least `capacity` bytes of buffer space.
    ///
    /// The initial backpressure boundary is `capacity`. The buffer grows as
    /// needed; zero is allowed and causes a flush before each frame is encoded.
    pub fn with_capacity(inner: W, encoder: E, capacity: usize) -> Self {
        Self {
            inner,
            encoder,
            buffer: BytesMut::with_capacity(capacity),
            backpressure_boundary: capacity,
        }
    }

    /// Returns a reference to the underlying writer.
    ///
    /// Writing directly to it bypasses pending frames.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Returns a mutable reference to the underlying writer.
    ///
    /// Writing directly to it bypasses pending frames.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Consumes this adapter and returns the underlying writer.
    ///
    /// Buffered output and encoder state are discarded. Call [`flush`](Self::flush)
    /// first to write pending frames.
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// Returns a reference to the encoder.
    pub fn encoder(&self) -> &E {
        &self.encoder
    }

    /// Returns a mutable reference to the encoder.
    pub fn encoder_mut(&mut self) -> &mut E {
        &mut self.encoder
    }

    /// Returns a reference to the pending output buffer.
    pub fn write_buffer(&self) -> &BytesMut {
        &self.buffer
    }

    /// Returns a mutable reference to the pending output buffer.
    ///
    /// This does not reset encoder state or change output already written.
    pub fn write_buffer_mut(&mut self) -> &mut BytesMut {
        &mut self.buffer
    }

    /// Returns the buffered byte threshold for flushing before the next frame.
    pub fn backpressure_boundary(&self) -> usize {
        self.backpressure_boundary
    }

    /// Sets the buffered byte threshold for flushing before the next frame.
    ///
    /// This does not flush immediately or limit the size of an encoded frame.
    pub fn set_backpressure_boundary(&mut self, boundary: usize) {
        self.backpressure_boundary = boundary;
    }

    /// Maps the encoder from `E` to `C`, preserving the writer, buffer, and boundary.
    pub fn map_encoder<C, F>(self, map: F) -> FramedWrite<W, C>
    where
        F: FnOnce(E) -> C,
    {
        FramedWrite {
            inner: self.inner,
            encoder: map(self.encoder),
            buffer: self.buffer,
            backpressure_boundary: self.backpressure_boundary,
        }
    }
}

impl<W: Write, E> FramedWrite<W, E> {
    /// Encodes a frame into the output buffer.
    ///
    /// Flushes first if buffered bytes have reached the backpressure boundary.
    /// Success does not mean the new frame has been written; call [`flush`](Self::flush)
    /// to write pending output.
    ///
    /// # Errors
    ///
    /// If flushing fails, `item` is not encoded. Encoding errors preserve buffer
    /// and encoder mutations. In either case, ownership of `item` is consumed.
    pub fn write<Item>(&mut self, item: Item) -> Result<(), E::Error>
    where
        E: Encoder<Item>,
    {
        // As with Tokio's poll_ready followed by start_send, drain a full buffer
        // before asking the encoder to append another frame. A failure here must
        // leave the encoder untouched; the buffer retains any unwritten suffix.
        if self.buffer.len() >= self.backpressure_boundary {
            self.flush()?;
        }

        // Encode directly into the pending output. The encoder owns the format
        // and any growth needed for this frame; no intermediate frame buffer is
        // required. Its mutations on failure are intentionally not rolled back.
        self.encoder.encode(item, &mut self.buffer)
    }

    /// Writes buffered bytes, then flushes the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns I/O errors without retrying, including [`io::ErrorKind::Interrupted`].
    /// A zero-byte write with pending output returns [`io::ErrorKind::WriteZero`].
    /// Unwritten bytes remain buffered; call this method again to resume.
    pub fn flush(&mut self) -> io::Result<()> {
        // Use individual writes rather than write_all: each success tells us
        // exactly how much output to remove before a possible later error.
        // Errors, including Interrupted, leave this iteration's bytes untouched.
        while !self.buffer.is_empty() {
            let written = self.inner.write(&self.buffer)?;
            // A return value of Ok(0) typically means that the underlying object is no longer able
            // to accept bytes and will likely not be able to in the future as well, or that the
            // buffer provided is empty.
            if written == 0 {
                // No progress with nonempty input cannot complete the flush.
                // Return with the pending bytes intact instead of looping forever.
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write frame to destination",
                ));
            }

            // Drop only the accepted prefix. Advancing the view does not copy
            // the suffix, and a retry will never submit this prefix again.
            self.buffer.advance(written);
        }

        // All our bytes have been accepted, but the destination may buffer them
        // itself. Even an empty buffer must reach this call. If it fails, another
        // flush retries the destination without re-encoding or resending bytes.
        self.inner.flush()
    }
}
