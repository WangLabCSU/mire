//! Byte streams with three independent readers.
//!
//! [`VecReader`] returns `Vec<u8>` and [`SliceReader`] lends `&[u8]`. Both accept
//! an existing [`std::io::BufRead`]. [`BytesReader`] buffers a [`std::io::Read`]
//! and returns independently held [`bytes::BytesMut`]. Each reader offers
//! exact-byte delimiters and LF/CRLF lines. No byte operation validates
//! UTF-8, converts to text, or trims whitespace.
//!
//! Empty input is `None`; an empty line is `Some(empty)`. Excluding a line
//! ending removes LF and one immediately preceding CR. Isolated CR and an
//! unterminated final line are preserved. I/O errors retain
//! undelivered bytes for a subsequent call. All byte methods and the standard
//! `Read` / `BufRead` methods share a position.
//!
//! ```
//! use std::io;
//! use mire_byteio::{BytesReader, VecReader, SliceReader, DelimPolicy};
//!
//! let input = b"\xff\0\r\nnext\n".as_slice();
//! let mut owned = VecReader::new(input);
//! let line: Vec<u8> = owned.read_line(DelimPolicy::Exclude)?.unwrap();
//! assert_eq!(line, b"\xff\0");
//!
//! let mut borrowed = SliceReader::new(input);
//! let line: &[u8] = borrowed.read_line(DelimPolicy::Exclude)?.unwrap();
//! assert_eq!(line, b"\xff\0");
//!
//! let mut shared = BytesReader::new(input);
//! let first = shared.read_line(DelimPolicy::Exclude)?.unwrap();
//! let second = shared.read_line(DelimPolicy::Exclude)?.unwrap();
//! drop(shared);
//! assert_eq!(first.as_ref(), b"\xff\0");
//! assert_eq!(second.as_ref(), b"next");
//! # Ok::<(), io::Error>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod bytes_reader;
mod slice_reader;
mod vec_reader;

pub use self::bytes_reader::BytesReader;
pub use self::slice_reader::SliceReader;
pub use self::vec_reader::VecReader;

/// Whether a delimited result includes its terminating byte or line ending.
///
/// Both policies consume the complete delimiter. An unterminated tail is returned
/// unchanged; a missing delimiter is never added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelimPolicy {
    /// Keep the delimiter when present.
    Include,
    /// Remove the delimiter when present.
    Exclude,
}

impl DelimPolicy {
    // The caller supplies a segment ending in its delimiter.
    fn delim_end(&self, bytes: &[u8], strip_cr: bool) -> usize {
        let mut end = bytes.len();
        match self {
            DelimPolicy::Include => {}
            DelimPolicy::Exclude => {
                if let Some(pos) = end.checked_sub(1) {
                    if strip_cr
                        && let Some(pos) = pos.checked_sub(1)
                        && let Some(b'\r') = bytes.get(pos)
                    {
                        end = pos;
                    } else {
                        end = pos;
                    }
                }
            }
        }
        end
    }
}

#[cfg(test)]
mod tests;
