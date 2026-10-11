use std::io;

use bytes::BytesMut;
use codec::{Encoder, FramedWrite};

// Each frame has a two-byte big-endian length followed by that many bytes.
struct LengthPrefixed;

impl<T: AsRef<[u8]>> Encoder<T> for LengthPrefixed {
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> io::Result<()> {
        let bytes = item.as_ref();
        // Validate before appending so an oversized frame leaves dst unchanged.
        let len = u16::try_from(bytes.len()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "frame exceeds 65535 bytes")
        })?;
        dst.reserve(2 + bytes.len());
        dst.extend_from_slice(&len.to_be_bytes());
        dst.extend_from_slice(bytes);
        Ok(())
    }
}

fn main() -> io::Result<()> {
    let mut writer = FramedWrite::with_capacity(Vec::new(), LengthPrefixed, 3);
    writer.write(b"\xff\0")?;
    writer.write(b"")?;
    writer.write(BytesMut::from(b"abc".as_slice()))?;

    // Flush the last buffered frame before taking the destination.
    writer.flush()?;
    assert_eq!(writer.into_inner(), b"\0\x02\xff\0\0\0\0\x03abc");
    Ok(())
}
