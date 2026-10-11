use std::io;

use bytes::BytesMut;

/// Encodes frames into a byte buffer.
///
/// Used with [`FramedWrite`](crate::FramedWrite). An encoder may support more
/// than one input type by implementing this trait for each type.
pub trait Encoder<Item> {
    /// Encoding failures, including errors from the output destination.
    type Error: From<io::Error>;

    /// Appends an encoded frame to `dst`.
    ///
    /// Preserve existing bytes in `dst` and reserve additional space as needed.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails. Changes to `dst` and encoder state
    /// are not rolled back.
    fn encode(&mut self, item: Item, dst: &mut BytesMut) -> Result<(), Self::Error>;
}
