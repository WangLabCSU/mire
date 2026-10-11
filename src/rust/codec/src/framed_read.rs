use std::io::Read;

use bytes::BytesMut;

use crate::Decoder;

const INITIAL_CAPACITY: usize = 8 * 1024;

/// An iterator of frames decoded from a [`Read`] source.
///
/// Frames are decoded before reading more input. See the [crate example](crate)
/// for usage with a [`Decoder`].
///
/// At EOF, [`Decoder::decode_eof`] supplies any remaining frames. Further calls
/// to `next` may resume reading; use [`Iterator::fuse`] to stop at the first `None`.
///
/// # Errors
///
/// Yields I/O and decoding errors without automatic retries, including
/// [`std::io::ErrorKind::Interrupted`]. Calling `next` again retries the failed
/// operation with its current state; errors do not roll back consumed input.
#[derive(Debug)]
pub struct FramedRead<R, D> {
    inner: R,
    decoder: D,
    buffer: BytesMut,
    // The most recent successful read returned zero bytes. A later read may
    // produce more input, so this does not permanently end the iterator.
    eof: bool,
    // Try decoding before reading again. The buffer need not contain a whole
    // frame, and the decoder may have a frame ready even when the buffer is empty.
    can_decode: bool,
}

impl<R, D> FramedRead<R, D> {
    /// Creates a reader using `decoder` and the default buffer capacity.
    ///
    /// The default is currently 8 KiB and may change in the future.
    pub fn new(inner: R, decoder: D) -> Self {
        Self::with_capacity(inner, decoder, INITIAL_CAPACITY)
    }

    /// Creates a reader using `decoder` and at least `capacity` bytes of buffer space.
    ///
    /// The buffer grows as needed. A capacity of zero is allowed.
    pub fn with_capacity(inner: R, decoder: D, capacity: usize) -> Self {
        Self {
            inner,
            decoder,
            buffer: BytesMut::with_capacity(capacity),
            eof: false,
            can_decode: false,
        }
    }

    /// Returns a reference to the underlying reader.
    ///
    /// Reading directly from it bypasses buffered input.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Returns a mutable reference to the underlying reader.
    ///
    /// Reading directly from it bypasses buffered input.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Consumes this adapter and returns the underlying reader.
    ///
    /// Any buffered input and decoder state are discarded.
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Returns a reference to the decoder.
    pub fn decoder(&self) -> &D {
        &self.decoder
    }

    /// Returns a mutable reference to the decoder.
    pub fn decoder_mut(&mut self) -> &mut D {
        &mut self.decoder
    }

    /// Returns a reference to the read buffer.
    pub fn read_buffer(&self) -> &BytesMut {
        &self.buffer
    }

    /// Returns a mutable reference to the read buffer.
    ///
    /// This does not reset decoding or EOF state.
    pub fn read_buffer_mut(&mut self) -> &mut BytesMut {
        &mut self.buffer
    }

    /// Maps the decoder from `D` to `C`, preserving the reader, buffer, and read state.
    pub fn map_decoder<C, F>(self, map: F) -> FramedRead<R, C>
    where
        F: FnOnce(D) -> C,
    {
        FramedRead {
            inner: self.inner,
            decoder: map(self.decoder),
            buffer: self.buffer,
            eof: self.eof,
            can_decode: self.can_decode,
        }
    }
}

impl<R: Read, D: Decoder> Iterator for FramedRead<R, D> {
    type Item = Result<D::Item, D::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // Both flags persist between calls, including calls that return an error.
        //
        // can_decode | eof   | next operation
        // -----------|-------|-----------------------------------------------
        // false      | false | Read more input.
        // true       | false | Decode buffered input or decoder-held frames.
        // true       | true  | Decode remaining frames at EOF.
        // false      | true  | EOF handling is complete; None has already been returned once. Check for new input.
        loop {
            if self.can_decode {
                // Drain available frames before reading again. At EOF, only
                // decode_eof decides whether another closing frame is available.
                let result = if self.eof {
                    self.decoder.decode_eof(&mut self.buffer)
                } else {
                    self.decoder.decode(&mut self.buffer)
                };

                match result {
                    // Keep can_decode set: another frame may already be ready,
                    // even if this one consumed the last buffered byte.
                    Ok(Some(frame)) => return Some(Ok(frame)),
                    // Preserve the current phase and let the caller decide
                    // whether to retry. Decoder mutations are not rolled back.
                    Err(error) => return Some(Err(error)),
                    // Do not call the decoder again until a read makes progress
                    // or reports EOF for the first time.
                    Ok(None) => self.can_decode = false,
                }

                if self.eof {
                    // decode_eof has finished. End this call without reading;
                    // a later next() may check whether the source has resumed.
                    return None;
                }
            }

            // A read into an empty slice can return zero without reaching EOF.
            // Reserve at least one byte, including when starting at capacity zero
            // or when an incomplete frame fills the current buffer.
            self.buffer.reserve(1);
            // Bytes before len belong to the unfinished frame and must survive
            // short reads, EOF, and I/O errors.
            let len = self.buffer.len();

            // Read requires initialized &mut [u8]. Temporarily extend the length
            // to expose the initialized tail, then trim it to the committed bytes
            // in each result branch. Reading here avoids an intermediate copy.
            // A panic skips that trimming; reuse after a panic is not supported.
            self.buffer.resize(self.buffer.capacity(), 0);
            match self.inner.read(&mut self.buffer[len..]) {
                Ok(0) => {
                    self.buffer.truncate(len);
                    if self.eof {
                        // EOF was already finalized and no new input arrived.
                        // Calling decode_eof again could repeat closing frames.
                        return None;
                    }
                    // This is the first EOF observation for the current input.
                    // Finalization must run even if there are no buffered bytes.
                    self.eof = true
                }
                Ok(nread) => {
                    // Commit only the bytes reported by Read; a short read is
                    // progress, not EOF. The unused initialized tail is removed.
                    self.buffer.truncate(len + nread);
                    // New input also resumes ordinary decoding after a temporary EOF.
                    self.eof = false
                }
                Err(error) => {
                    // Keep the prefix from earlier successful reads. can_decode
                    // remains false, so a caller retry attempts another read.
                    // Interrupted follows this same path; retries are not automatic.
                    self.buffer.truncate(len);
                    return Some(Err(error.into()));
                }
            }

            // Try decode after new input, or decode_eof after the first zero-byte
            // read. Keep this set across returned frames until decoding says None.
            self.can_decode = true;
        }
    }
}
