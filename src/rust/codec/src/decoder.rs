use std::io;

use bytes::BytesMut;

/// Decodes frames from a byte buffer.
///
/// Used with [`FramedRead`](crate::FramedRead). Implementations may keep state
/// between calls. See the [crate example](crate) for usage.
pub trait Decoder {
    /// The type of a decoded frame.
    type Item;

    /// Decoding failures, including errors from the input source.
    type Error: From<io::Error>;

    /// Attempts to decode one frame from `src`.
    ///
    /// Returns `Ok(Some(frame))` on success, or `Ok(None)` if more input is needed.
    /// After returning a frame, this may be called again with no additional
    /// input, including an empty `src`.
    ///
    /// Remove consumed bytes from `src`. Partial data may be kept in `src` or
    /// in the decoder. Reserve space in `src` as needed for subsequent reads.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding fails. Changes to `src` and decoder state
    /// are not rolled back.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error>;

    /// Decodes remaining frames at EOF.
    ///
    /// Called until it returns `Ok(None)`, even when `src` is empty. Once `None`
    /// is returned, this is not called again until more input has been read.
    ///
    /// Override this for different framing rules at EOF or to check partial
    /// data held in the decoder.
    ///
    /// # Errors
    ///
    /// The default calls [`decode`](Self::decode), returning its result or an
    /// [`io::ErrorKind::Other`] error if it returns `Ok(None)` with bytes left
    /// in `src`.
    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Give the decoder a chance to produce a complete frame first. The
        // reader calls this again after each frame to finish draining the input.
        match self.decode(src)? {
            Some(frame) => Ok(Some(frame)),
            // This checks only the input buffer. Decoders that store partial
            // frames internally must override this method to validate that state.
            None if src.is_empty() => Ok(None),
            // Report trailing bytes instead of declaring decoding complete.
            None => Err(io::Error::other("bytes remaining on stream").into()),
        }
    }
}
