//! Encoding and decoding frames in byte streams.
//!
//! Implement [`Decoder`] to define a frame format, then use [`FramedRead`] to
//! read frames from a [`std::io::Read`] source as an iterator.
//! Implement [`Encoder`] and use [`FramedWrite`] to write frames to a
//! [`std::io::Write`] destination.
//!
//! # Examples
//!
//! Reading pairs of bytes:
//!
//! ```
//! use std::io;
//!
//! use bytes::BytesMut;
//! use codec::{Decoder, FramedRead};
//!
//! struct Pairs;
//!
//! impl Decoder for Pairs {
//!     type Item = BytesMut;
//!     type Error = io::Error;
//!
//!     fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
//!         if src.len() < 2 {
//!             return Ok(None);
//!         }
//!         Ok(Some(src.split_to(2)))
//!     }
//! }
//!
//! let frames = FramedRead::new(b"ab\xff\0".as_slice(), Pairs)
//!     .collect::<io::Result<Vec<_>>>()?;
//! assert_eq!(frames[0].as_ref(), b"ab");
//! assert_eq!(frames[1].as_ref(), b"\xff\0");
//! # Ok::<(), io::Error>(())
//! ```

#![forbid(unsafe_code)]

mod decoder;
mod encoder;
mod framed_read;
mod framed_write;

pub use self::decoder::Decoder;
pub use self::encoder::Encoder;
pub use self::framed_read::FramedRead;
pub use self::framed_write::FramedWrite;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod write_tests;
