use std::{
    collections::VecDeque,
    io::{self, Cursor, ErrorKind, Read},
};

use bytes::{Buf, BytesMut};

use crate::{Decoder, FramedRead};

#[derive(Debug)]
struct Pairs;

impl Decoder for Pairs {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        Ok((src.len() >= 2).then(|| src.split_to(2)))
    }
}

enum Step {
    Data(&'static [u8]),
    Error(ErrorKind),
    DirtyError,
    Panic,
    Overreport,
    Eof,
}

struct Script {
    steps: VecDeque<Step>,
    reads: usize,
    destinations: Vec<usize>,
}

impl Script {
    fn new(steps: impl IntoIterator<Item = Step>) -> Self {
        Self {
            steps: steps.into_iter().collect(),
            reads: 0,
            destinations: Vec::new(),
        }
    }
}

impl Read for Script {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        assert!(!destination.is_empty(), "a zero-sized read can mimic EOF");
        self.reads += 1;
        self.destinations.push(destination.as_ptr() as usize);
        match self.steps.pop_front().unwrap_or(Step::Eof) {
            Step::Data(bytes) => {
                let count = destination.len().min(bytes.len());
                destination[..count].copy_from_slice(&bytes[..count]);
                if count < bytes.len() {
                    self.steps.push_front(Step::Data(&bytes[count..]));
                }
                Ok(count)
            }
            Step::Error(kind) => Err(io::Error::from(kind)),
            Step::DirtyError => {
                destination.fill(0xee);
                Err(io::Error::from(ErrorKind::Other))
            }
            Step::Panic => {
                destination.fill(0xee);
                panic!("source panicked after modifying the destination")
            }
            Step::Overreport => Ok(destination.len() + 1),
            Step::Eof => Ok(0),
        }
    }
}

#[test]
fn default_eof_rejects_an_incomplete_frame() {
    let mut reader = FramedRead::new(Script::new([Step::Data(b"x")]), Pairs);
    assert_eq!(reader.next().unwrap().unwrap_err().kind(), ErrorKind::Other);
    assert_eq!(reader.read_buffer().as_ref(), b"x");
    assert_eq!(reader.next().unwrap().unwrap_err().kind(), ErrorKind::Other);
    assert_eq!(reader.get_ref().reads, 2);
}

#[test]
fn eof_decoding_can_be_retried_after_repairing_the_buffer() {
    let mut reader = FramedRead::new(Script::new([Step::Data(b"a")]), Pairs);
    assert_eq!(reader.next().unwrap().unwrap_err().kind(), ErrorKind::Other);
    reader.read_buffer_mut().extend_from_slice(b"b");
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert_eq!(reader.get_ref().reads, 2);
    assert!(reader.next().is_none());
}

#[test]
fn empty_input_only_calls_decode_eof_and_emits_all_closing_frames() {
    struct Closing(usize);

    impl Decoder for Closing {
        type Item = usize;
        type Error = io::Error;

        fn decode(&mut self, _: &mut BytesMut) -> io::Result<Option<usize>> {
            panic!("no ordinary decode before input arrives")
        }

        fn decode_eof(&mut self, _: &mut BytesMut) -> io::Result<Option<usize>> {
            if self.0 == 0 {
                return Ok(None);
            }
            self.0 -= 1;
            Ok(Some(self.0))
        }
    }

    let mut reader = FramedRead::new(io::empty(), Closing(2));
    assert_eq!(reader.next().unwrap().unwrap(), 1);
    assert_eq!(reader.next().unwrap().unwrap(), 0);
    assert!(reader.next().is_none());
    assert!(reader.next().is_none());
}

#[test]
fn decoder_errors_can_repeat_when_the_caller_continues() {
    struct Invalid;

    impl Decoder for Invalid {
        type Item = ();
        type Error = io::Error;

        fn decode(&mut self, _: &mut BytesMut) -> io::Result<Option<()>> {
            Err(io::Error::from(ErrorKind::InvalidData))
        }
    }

    let mut reader = FramedRead::new(Script::new([Step::Data(b"x")]), Invalid);
    for _ in 0..2 {
        assert_eq!(
            reader.next().unwrap().unwrap_err().kind(),
            ErrorKind::InvalidData
        );
        assert_eq!(reader.get_ref().reads, 1);
        assert_eq!(reader.read_buffer().as_ref(), b"x");
    }
}

#[test]
fn interrupted_reads_are_returned_for_the_caller_to_retry() {
    let source = Script::new([
        Step::Error(ErrorKind::Interrupted),
        Step::Data(b"a"),
        Step::Error(ErrorKind::Interrupted),
        Step::Data(b"b"),
    ]);
    let mut reader = FramedRead::new(source, Pairs);
    assert_eq!(
        reader.next().unwrap().unwrap_err().kind(),
        ErrorKind::Interrupted
    );
    assert!(reader.read_buffer().is_empty());
    assert_eq!(reader.get_ref().reads, 1);
    assert_eq!(
        reader.next().unwrap().unwrap_err().kind(),
        ErrorKind::Interrupted
    );
    assert_eq!(reader.read_buffer().as_ref(), b"a");
    assert_eq!(reader.get_ref().reads, 3);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert!(reader.next().is_none());
}

#[test]
fn reading_can_resume_after_a_temporary_eof() {
    let source = Script::new([Step::Data(b"ab"), Step::Eof, Step::Eof, Step::Data(b"cd")]);
    let mut reader = FramedRead::new(source, Pairs);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert!(reader.next().is_none());
    assert!(reader.next().is_none());
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"cd");
    assert!(reader.next().is_none());
}

#[test]
fn buffered_frames_are_drained_before_reading_more() {
    let mut reader = FramedRead::new(Script::new([Step::Data(b"abcdef")]), Pairs);
    assert_eq!(reader.get_ref().reads, 0);
    for expected in [b"ab", b"cd", b"ef"] {
        assert_eq!(reader.next().unwrap().unwrap().as_ref(), expected);
        assert_eq!(reader.get_ref().reads, 1);
    }
    assert!(reader.next().is_none());
    assert_eq!(reader.get_ref().reads, 2);
}

#[test]
fn a_pending_decode_is_not_repeated_without_new_input() {
    #[derive(Default)]
    struct CountedPairs(usize);

    impl Decoder for CountedPairs {
        type Item = BytesMut;
        type Error = io::Error;

        fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
            self.0 += 1;
            Pairs.decode(src)
        }
    }

    let source = Script::new([
        Step::Data(b"a"),
        Step::Error(ErrorKind::Interrupted),
        Step::Data(b"b"),
    ]);
    let mut reader = FramedRead::new(source, CountedPairs::default());
    assert_eq!(
        reader.next().unwrap().unwrap_err().kind(),
        ErrorKind::Interrupted
    );
    assert_eq!(reader.decoder().0, 1);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert_eq!(reader.decoder().0, 2);
}

#[test]
fn eof_finalization_is_not_repeated_until_new_input_arrives() {
    struct CountedEof(usize);

    impl Decoder for CountedEof {
        type Item = BytesMut;
        type Error = io::Error;

        fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
            Pairs.decode(src)
        }

        fn decode_eof(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
            self.0 += 1;
            Pairs.decode_eof(src)
        }
    }

    let source = Script::new([Step::Eof, Step::Eof, Step::Data(b"ab"), Step::Eof]);
    let mut reader = FramedRead::new(source, CountedEof(0));
    assert!(reader.next().is_none());
    assert_eq!(reader.decoder().0, 1);
    assert!(reader.next().is_none());
    assert_eq!(reader.decoder().0, 1);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert!(reader.next().is_none());
    assert_eq!(reader.decoder().0, 2);
}

#[test]
fn io_errors_retain_a_prefix_for_the_next_read_attempt() {
    let source = Script::new([
        Step::Data(b"a"),
        Step::Error(ErrorKind::WouldBlock),
        Step::Data(b"b"),
    ]);
    let mut reader = FramedRead::new(source, Pairs);
    assert_eq!(
        reader.next().unwrap().unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(reader.read_buffer().as_ref(), b"a");
    assert_eq!(reader.get_ref().reads, 2);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert_eq!(reader.get_ref().reads, 3);
    assert!(reader.next().is_none());
}

#[test]
fn decoder_changes_are_not_rolled_back_after_an_error() {
    struct FailOnce(bool);

    impl Decoder for FailOnce {
        type Item = BytesMut;
        type Error = io::Error;

        fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
            if self.0 {
                self.0 = false;
                src.advance(1);
                return Err(io::Error::from(ErrorKind::InvalidData));
            }
            Pairs.decode(src)
        }
    }

    let source = Script::new([Step::Data(b"xab"), Step::Data(b"cd")]);
    let mut reader = FramedRead::new(source, FailOnce(true));
    assert_eq!(
        reader.next().unwrap().unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(reader.read_buffer().as_ref(), b"ab");
    assert!(!reader.decoder().0);
    assert_eq!(reader.get_ref().reads, 1);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert_eq!(reader.get_ref().reads, 1);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"cd");
    assert!(reader.next().is_none());
}

#[test]
fn reads_target_the_buffer_tail_and_split_frames_keep_the_allocation() {
    let source = Script::new([Step::Data(b"a"), Step::Data(b"b")]);
    let mut reader = FramedRead::with_capacity(source, Pairs, 8);
    let start = reader.read_buffer().as_ptr() as usize;
    let frame = reader.next().unwrap().unwrap();
    assert_eq!(reader.get_ref().destinations, [start, start + 1]);
    assert_eq!(frame.as_ptr() as usize, start);
    assert_eq!(frame.as_ref(), b"ab");
}

#[test]
fn failed_reads_do_not_expose_modified_tail_bytes() {
    let source = Script::new([Step::Data(b"a"), Step::DirtyError, Step::Data(b"b")]);
    let mut reader = FramedRead::with_capacity(source, Pairs, 8);
    assert!(reader.next().unwrap().is_err());
    assert_eq!(reader.read_buffer().as_ref(), b"a");
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert!(reader.next().is_none());
}

#[test]
#[should_panic(expected = "source panicked after modifying the destination")]
fn source_panics_propagate_to_the_caller() {
    let source = Script::new([Step::Data(b"a"), Step::Panic]);
    let mut reader = FramedRead::with_capacity(source, Pairs, 8);
    let _ = reader.next();
}

#[test]
fn a_read_count_larger_than_the_destination_is_rejected() {
    let source = Script::new([Step::Data(b"a"), Step::Overreport, Step::Data(b"b")]);
    let mut reader = FramedRead::with_capacity(source, Pairs, 8);
    assert_eq!(
        reader.next().unwrap().unwrap_err().kind(),
        ErrorKind::InvalidData
    );
    assert_eq!(reader.read_buffer().as_ref(), b"a");
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
}

struct LengthPrefixed;

impl Decoder for LengthPrefixed {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
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

struct ShortReads<R> {
    source: R,
    limit: usize,
}

impl<R: Read> Read for ShortReads<R> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        let len = destination.len().min(self.limit);
        self.source.read(&mut destination[..len])
    }
}

#[test]
fn empty_binary_and_long_frames_survive_short_reads_and_retention() {
    let expected = [
        Vec::new(),
        vec![0, 255, 13, 10],
        (0..=255).cycle().take(20_000).collect(),
        Vec::new(),
        b"tail".to_vec(),
    ];
    let wire: Vec<_> = expected
        .iter()
        .flat_map(|frame| {
            let len = u16::try_from(frame.len()).unwrap().to_be_bytes();
            len.into_iter().chain(frame.iter().copied())
        })
        .collect();

    for capacity in [0, 1, 2, 7, 8192] {
        for limit in [1, 3, 1024, 30_000] {
            let source = ShortReads {
                source: Cursor::new(&wire),
                limit,
            };
            let frames = FramedRead::with_capacity(source, LengthPrefixed, capacity)
                .collect::<io::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(frames.len(), expected.len());
            for (frame, expected) in frames.iter().zip(&expected) {
                assert_eq!(frame.as_ref(), expected);
            }
        }
    }
}

#[test]
fn incomplete_length_header_and_payload_are_errors_at_eof() {
    for wire in [b"\0".as_slice(), b"\0\x02x".as_slice()] {
        let mut reader = FramedRead::with_capacity(wire, LengthPrefixed, 1);
        assert_eq!(reader.next().unwrap().unwrap_err().kind(), ErrorKind::Other);
        assert_eq!(reader.read_buffer().as_ref(), wire);
    }
}

#[test]
fn a_decoder_can_emit_a_frame_from_internal_state_at_eof() {
    #[derive(Default)]
    struct UntilEof(BytesMut);

    impl Decoder for UntilEof {
        type Item = BytesMut;
        type Error = io::Error;

        fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
            self.0.extend_from_slice(src);
            src.clear();
            Ok(None)
        }

        fn decode_eof(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
            assert!(src.is_empty());
            Ok((!self.0.is_empty()).then(|| self.0.split()))
        }
    }

    let source = Script::new([Step::Data(b"a"), Step::Data(b"b")]);
    let mut reader = FramedRead::new(source, UntilEof::default());
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert!(reader.next().is_none());
}

#[test]
fn mapping_a_decoder_preserves_pending_input_after_a_read_error() {
    let source = Script::new([
        Step::Data(b"a"),
        Step::Error(ErrorKind::WouldBlock),
        Step::Data(b"b"),
    ]);
    let mut reader = FramedRead::new(source, Pairs);
    assert!(reader.next().unwrap().is_err());
    let mut reader = reader.map_decoder(|_| Pairs);
    assert_eq!(reader.read_buffer().as_ref(), b"a");
    assert_eq!(reader.get_ref().reads, 2);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert_eq!(reader.get_ref().reads, 3);
}

#[test]
fn mapping_a_decoder_preserves_buffered_frames_and_eof_state() {
    let mut reader = FramedRead::new(Script::new([Step::Data(b"abcd")]), Pairs);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    let mut reader = reader.map_decoder(|_| Pairs);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"cd");
    assert_eq!(reader.get_ref().reads, 1);
    assert!(reader.next().is_none());
    let mut reader = reader.map_decoder(|_| Pairs);
    assert!(reader.next().is_none());
    assert_eq!(reader.get_ref().reads, 3);
}

#[test]
fn frames_can_also_be_decoded_from_a_borrowed_source() {
    let mut source = Cursor::new(b"abcdef");
    let mut reader = FramedRead::with_capacity(&mut source, Pairs, 2);
    assert_eq!(reader.next().unwrap().unwrap().as_ref(), b"ab");
    assert_eq!(reader.into_inner().position(), 2);
    assert_eq!(source.position(), 2);
}
