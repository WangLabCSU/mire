use std::io::{self, BufReader, Read};

use bytes::BytesMut;
use mire_byteio::{BytesReader, DelimPolicy, SliceReader, VecReader};

fn main() -> io::Result<()> {
    let input = b"\xff\0\r\nnext\nRAW|tail".as_slice();

    let mut reader = VecReader::new(BufReader::with_capacity(4, input));
    let first: Vec<u8> = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    let second: Vec<u8> = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    drop(reader);
    assert_eq!(first, b"\xff\0");
    assert_eq!(second, b"next");

    let mut reader = SliceReader::new(BufReader::with_capacity(4, input));
    let first: &[u8] = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    assert_eq!(first, b"\xff\0");
    // The previous slice's last use precedes the next mutable read.
    let second: &[u8] = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    assert_eq!(second, b"next");
    let mut raw = [0; 3];
    reader.read_exact(&mut raw)?;
    assert_eq!(&raw, b"RAW");
    assert_eq!(
        reader.read_delim(b'|', DelimPolicy::Exclude)?,
        Some(b"".as_slice())
    );
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude)?,
        Some(b"tail".as_slice())
    );
    assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());

    let mut reader = BytesReader::with_capacity(4, input);
    let first: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    let second: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    drop(reader);
    assert_eq!(first.as_ref(), b"\xff\0");
    assert_eq!(second.as_ref(), b"next");
    Ok(())
}
