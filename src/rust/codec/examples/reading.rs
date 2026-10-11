use std::io;

use bytes::{Buf, BytesMut};
use codec::{Decoder, FramedRead};

// Each frame has a two-byte big-endian length followed by that many bytes.
struct LengthPrefixed;

impl Decoder for LengthPrefixed {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Self::Item>> {
        if src.len() < 2 {
            return Ok(None);
        }
        let len = u16::from_be_bytes([src[0], src[1]]) as usize;
        if src.len() < 2 + len {
            src.reserve(2 + len - src.len());
            return Ok(None);
        }
        src.advance(2);
        Ok(Some(src.split_to(len)))
    }
}

fn main() -> io::Result<()> {
    let input = b"\0\x02\xff\0\0\0\0\x03abc";
    let mut reader = FramedRead::with_capacity(input.as_slice(), LengthPrefixed, 3);

    // Each frame can outlive subsequent reads and the reader itself.
    let first = reader.next().transpose()?.expect("first frame");
    let empty = reader.next().transpose()?.expect("empty frame");
    let last = reader.next().transpose()?.expect("last frame");
    assert!(reader.next().is_none());
    drop(reader);

    assert_eq!(first.as_ref(), b"\xff\0");
    assert!(empty.is_empty());
    assert_eq!(last.as_ref(), b"abc");
    Ok(())
}
