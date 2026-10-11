use std::{
    collections::VecDeque,
    io::{self, ErrorKind, Write},
};

use bytes::{Buf, BytesMut};

use crate::{Decoder, Encoder, FramedRead, FramedWrite};

#[derive(Debug, Default)]
struct LengthPrefixed {
    frames: usize,
}

impl<T: AsRef<[u8]>> Encoder<T> for LengthPrefixed {
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> io::Result<()> {
        let bytes = item.as_ref();
        let len =
            u16::try_from(bytes.len()).map_err(|_| io::Error::from(ErrorKind::InvalidInput))?;
        dst.reserve(2 + bytes.len());
        dst.extend_from_slice(&len.to_be_bytes());
        dst.extend_from_slice(bytes);
        self.frames += 1;
        Ok(())
    }
}

impl Decoder for LengthPrefixed {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        if src.len() < 2 {
            return Ok(None);
        }
        let len = u16::from_be_bytes([src[0], src[1]]) as usize;
        if src.len() < 2 + len {
            return Ok(None);
        }
        src.advance(2);
        Ok(Some(src.split_to(len)))
    }
}

enum Step {
    Accept(usize),
    Error(ErrorKind),
}

#[derive(Default)]
struct Script {
    steps: VecDeque<Step>,
    flush_errors: VecDeque<ErrorKind>,
    output: Vec<u8>,
    writes: usize,
    flushes: usize,
    sources: Vec<usize>,
}

impl Script {
    fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
            ..Self::default()
        }
    }
}

impl Write for Script {
    fn write(&mut self, source: &[u8]) -> io::Result<usize> {
        assert!(!source.is_empty());
        self.writes += 1;
        self.sources.push(source.as_ptr() as usize);
        match self.steps.pop_front().unwrap_or(Step::Accept(source.len())) {
            Step::Accept(limit) => {
                let count = limit.min(source.len());
                self.output.extend_from_slice(&source[..count]);
                Ok(count)
            }
            Step::Error(kind) => Err(io::Error::from(kind)),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        if let Some(kind) = self.flush_errors.pop_front() {
            return Err(io::Error::from(kind));
        }
        Ok(())
    }
}

#[test]
fn frames_are_batched_until_the_boundary_or_an_explicit_flush() {
    let mut writer = FramedWrite::with_capacity(Script::default(), LengthPrefixed::default(), 4);
    writer.write(b"ab").unwrap();
    assert!(writer.get_ref().output.is_empty());
    assert_eq!(writer.write_buffer().as_ref(), b"\0\x02ab");

    writer.write(b"c").unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x02ab");
    assert_eq!(writer.get_ref().flushes, 1);
    assert_eq!(writer.write_buffer().as_ref(), b"\0\x01c");

    writer.flush().unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x02ab\0\x01c");
    assert_eq!(writer.get_ref().flushes, 2);
    assert!(writer.write_buffer().is_empty());
}

#[test]
fn short_writes_use_the_remaining_buffer_without_repeating_bytes() {
    let source = Script::new([Step::Accept(1), Step::Accept(2), Step::Accept(1)]);
    let mut writer = FramedWrite::new(source, LengthPrefixed::default());
    writer.write(b"\xff\0abc").unwrap();
    let start = writer.write_buffer().as_ptr() as usize;
    writer.flush().unwrap();

    assert_eq!(writer.get_ref().output, b"\0\x05\xff\0abc");
    assert_eq!(
        writer.get_ref().sources,
        [start, start + 1, start + 3, start + 4]
    );
    assert_eq!(writer.get_ref().flushes, 1);
}

#[test]
fn io_errors_preserve_the_unwritten_suffix_for_a_flush_retry() {
    for kind in [
        ErrorKind::Interrupted,
        ErrorKind::WouldBlock,
        ErrorKind::Other,
    ] {
        let source = Script::new([Step::Accept(3), Step::Error(kind)]);
        let mut writer = FramedWrite::new(source, LengthPrefixed::default());
        writer.write(b"abcd").unwrap();

        assert_eq!(writer.flush().unwrap_err().kind(), kind);
        assert_eq!(writer.get_ref().writes, 2);
        assert_eq!(writer.get_ref().flushes, 0);
        assert_eq!(writer.get_ref().output, b"\0\x04a");
        assert_eq!(writer.write_buffer().as_ref(), b"bcd");

        writer.flush().unwrap();
        assert_eq!(writer.get_ref().output, b"\0\x04abcd");
        assert_eq!(writer.encoder().frames, 1);
        assert!(writer.write_buffer().is_empty());
    }
}

#[test]
fn zero_writes_return_write_zero_and_preserve_the_remaining_bytes() {
    let source = Script::new([Step::Accept(1), Step::Accept(0)]);
    let mut writer = FramedWrite::new(source, LengthPrefixed::default());
    writer.write(b"xy").unwrap();

    assert_eq!(writer.flush().unwrap_err().kind(), ErrorKind::WriteZero);
    assert_eq!(writer.get_ref().output, b"\0");
    assert_eq!(writer.write_buffer().as_ref(), b"\x02xy");
    writer.flush().unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x02xy");
}

#[test]
fn an_underlying_flush_error_does_not_requeue_written_bytes() {
    let source = Script {
        flush_errors: [ErrorKind::Interrupted].into(),
        ..Script::default()
    };
    let mut writer = FramedWrite::new(source, LengthPrefixed::default());
    writer.write(b"a").unwrap();

    assert_eq!(writer.flush().unwrap_err().kind(), ErrorKind::Interrupted);
    assert!(writer.write_buffer().is_empty());
    assert_eq!(writer.get_ref().flushes, 1);
    writer.flush().unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x01a");
    assert_eq!(writer.get_ref().writes, 1);
    assert_eq!(writer.get_ref().flushes, 2);
}

#[test]
fn flushing_an_empty_buffer_still_flushes_the_underlying_writer() {
    let mut writer = FramedWrite::new(Script::default(), LengthPrefixed::default());
    writer.flush().unwrap();
    assert_eq!(writer.get_ref().writes, 0);
    assert_eq!(writer.get_ref().flushes, 1);
}

#[test]
fn a_failed_boundary_flush_does_not_encode_the_new_frame() {
    let source = Script::new([Step::Accept(1), Step::Error(ErrorKind::WouldBlock)]);
    let mut writer = FramedWrite::with_capacity(source, LengthPrefixed::default(), 3);
    writer.write(b"a").unwrap();
    assert_eq!(
        writer.write(b"b").unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(writer.encoder().frames, 1);
    assert_eq!(writer.write_buffer().as_ref(), b"\x01a");

    writer.flush().unwrap();
    writer.write(b"b").unwrap();
    writer.flush().unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x01a\0\x01b");
}

#[test]
fn a_failed_underlying_boundary_flush_does_not_encode_the_new_frame() {
    let source = Script {
        flush_errors: [ErrorKind::Other].into(),
        ..Script::default()
    };
    let mut writer = FramedWrite::with_capacity(source, LengthPrefixed::default(), 3);
    writer.write(b"a").unwrap();
    assert_eq!(writer.write(b"b").unwrap_err().kind(), ErrorKind::Other);
    assert_eq!(writer.encoder().frames, 1);
    assert!(writer.write_buffer().is_empty());
    writer.flush().unwrap();
    writer.write(b"b").unwrap();
    writer.flush().unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x01a\0\x01b");
}

#[test]
fn zero_capacity_flushes_before_each_frame_and_can_still_encode() {
    let mut writer = FramedWrite::with_capacity(Script::default(), LengthPrefixed::default(), 0);
    writer.write(b"a").unwrap();
    assert_eq!(writer.get_ref().flushes, 1);
    assert!(writer.get_ref().output.is_empty());
    writer.write(b"b").unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x01a");
    writer.flush().unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x01a\0\x01b");
}

#[test]
fn long_empty_and_binary_frames_round_trip_with_short_writes() {
    let source = Script::new((0..32).map(|_| Step::Accept(1)));
    let mut writer = FramedWrite::with_capacity(source, LengthPrefixed::default(), 3);
    let expected = [vec![], vec![0xff, 0, b'\r', b'\n'], vec![0x80; 32 * 1024]];
    for frame in &expected {
        writer.write(frame).unwrap();
    }
    assert!(writer.write_buffer().len() > writer.backpressure_boundary());
    assert_eq!(writer.backpressure_boundary(), 3);
    writer.flush().unwrap();

    let source = writer.into_inner();
    let actual = FramedRead::with_capacity(source.output.as_slice(), LengthPrefixed::default(), 1)
        .collect::<io::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(&expected) {
        assert_eq!(actual.as_ref(), expected);
    }
}

#[test]
fn one_encoder_accepts_multiple_input_types_without_annotating_flush() {
    let mut writer = FramedWrite::new(Vec::new(), LengthPrefixed::default());
    writer.write(b"a".as_slice()).unwrap();
    writer.write(vec![b'b']).unwrap();
    writer.write(BytesMut::from(b"c".as_slice())).unwrap();
    writer.flush().unwrap();
    assert_eq!(writer.into_inner(), b"\0\x01a\0\x01b\0\x01c");
}

#[test]
fn encoding_errors_preserve_encoder_and_buffer_mutations() {
    struct Failing(bool);

    impl Encoder<()> for Failing {
        type Error = io::Error;

        fn encode(&mut self, _: (), dst: &mut BytesMut) -> io::Result<()> {
            self.0 = true;
            dst.extend_from_slice(b"partial");
            Err(io::Error::from(ErrorKind::InvalidData))
        }
    }

    let mut writer = FramedWrite::new(Script::default(), Failing(false));
    writer.write_buffer_mut().extend_from_slice(b"previous");
    assert_eq!(writer.write(()).unwrap_err().kind(), ErrorKind::InvalidData);
    assert!(writer.encoder().0);
    assert_eq!(writer.write_buffer().as_ref(), b"previouspartial");
    assert_eq!(writer.get_ref().writes, 0);

    // The caller can remove a failed encoding's suffix before flushing.
    writer.write_buffer_mut().truncate(b"previous".len());
    writer.encoder_mut().0 = false;
    writer.flush().unwrap();
    assert_eq!(writer.get_ref().output, b"previous");
}

#[test]
fn mapping_an_encoder_preserves_the_pending_suffix_and_boundary() {
    let source = Script::new([Step::Accept(1), Step::Error(ErrorKind::Other)]);
    let mut writer = FramedWrite::with_capacity(source, LengthPrefixed::default(), 4);
    writer.write(b"a").unwrap();
    writer.flush().unwrap_err();
    writer.set_backpressure_boundary(2);
    let mut writer = writer.map_encoder(|encoder| {
        assert_eq!(encoder.frames, 1);
        LengthPrefixed::default()
    });

    assert_eq!(writer.backpressure_boundary(), 2);
    assert_eq!(writer.write_buffer().as_ref(), b"\x01a");
    writer.write(b"b").unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x01a");
    writer.flush().unwrap();
    assert_eq!(writer.get_ref().output, b"\0\x01a\0\x01b");
}

#[test]
fn dropping_or_unwrapping_a_writer_does_not_flush_pending_frames() {
    let mut output = Script::default();
    {
        let mut writer = FramedWrite::new(&mut output, LengthPrefixed::default());
        writer.write(b"a").unwrap();
    }
    assert_eq!(output.writes, 0);
    assert_eq!(output.flushes, 0);

    let mut writer = FramedWrite::new(output, LengthPrefixed::default());
    writer.write(b"b").unwrap();
    let output = writer.into_inner();
    assert_eq!(output.writes, 0);
    assert_eq!(output.flushes, 0);
}

#[test]
fn borrowed_writers_receive_frames_after_flushing() {
    let mut output = Vec::new();
    {
        let mut writer = FramedWrite::new(&mut output, LengthPrefixed::default());
        writer.write(b"a").unwrap();
        writer.flush().unwrap();
    }
    assert_eq!(output, b"\0\x01a");
}
