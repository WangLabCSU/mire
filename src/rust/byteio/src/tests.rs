use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, ErrorKind, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};

use bytes::BytesMut;

use crate::{BytesReader, DelimPolicy, SliceReader, VecReader};

fn bytes<T: AsRef<[u8]> + ?Sized>(value: &T) -> &[u8] {
    value.as_ref()
}

// Expand the same contract against each concrete API without adding a public
// abstraction just for tests. Each expansion receives a fresh source.
macro_rules! check_readers {
    ($capacity:expr, $source:expr, |$reader:ident| $body:block) => {{
        {
            let mut $reader = VecReader::new(BufReader::with_capacity($capacity, $source));
            $body
        }
        {
            let mut $reader = SliceReader::new(BufReader::with_capacity($capacity, $source));
            $body
        }
        {
            let mut $reader = BytesReader::with_capacity($capacity, $source);
            $body
        }
    }};
}

#[test]
fn line_contract_holds_at_every_buffer_boundary() -> io::Result<()> {
    let input = b"\xff\r\n\n\r\nlast\r";
    for capacity in 1..=input.len() + 1 {
        check_readers!(capacity, input.as_slice(), |reader| {
            for expected in [b"\xff".as_slice(), b"", b"", b"last\r"] {
                assert_eq!(
                    bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                    expected
                );
            }
            assert!(reader.read_line(DelimPolicy::Include)?.is_none());
        });
    }
    Ok(())
}

#[test]
fn empty_input_empty_lines_and_trailing_delimiters_are_distinct() -> io::Result<()> {
    for capacity in 1..=4 {
        check_readers!(capacity, b"".as_slice(), |reader| {
            assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        });
        check_readers!(capacity, b"\n\r\n\n".as_slice(), |reader| {
            for _ in 0..3 {
                assert!(bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()).is_empty());
            }
            assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        });
    }
    Ok(())
}

#[test]
fn line_and_exact_delimiter_apply_only_their_own_ending_policy() -> io::Result<()> {
    for capacity in 1..=20 {
        let input = b"a\r\nb\r\nc\r\r\nd|last\r".as_slice();
        check_readers!(capacity, input, |reader| {
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Include)?.unwrap()),
                b"a\r\n"
            );
            assert_eq!(
                bytes(&reader.read_delim(b'\n', DelimPolicy::Exclude)?.unwrap()),
                b"b\r"
            );
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"c\r"
            );
            assert_eq!(
                bytes(&reader.read_delim(b'|', DelimPolicy::Include)?.unwrap()),
                b"d|"
            );
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"last\r"
            );
            assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        });
    }
    Ok(())
}

#[test]
fn line_ending_exclusion_preserves_a_cr_before_the_final_content_byte() -> io::Result<()> {
    let input = b"a\rb\nx\r\r\nlast\r";
    for capacity in 1..=input.len() + 1 {
        check_readers!(capacity, input.as_slice(), |reader| {
            for expected in [b"a\rb".as_slice(), b"x\r", b"last\r"] {
                assert_eq!(
                    bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                    expected
                );
            }
            assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        });
    }
    Ok(())
}

#[test]
fn every_byte_can_be_an_exact_delimiter() -> io::Result<()> {
    for delim in 0..=255_u8 {
        let value = delim.wrapping_add(1);
        let input = [delim, value, delim, value];
        for capacity in 1..=5 {
            check_readers!(capacity, input.as_slice(), |reader| {
                assert!(
                    bytes(&reader.read_delim(delim, DelimPolicy::Exclude)?.unwrap()).is_empty()
                );
                assert_eq!(
                    bytes(&reader.read_delim(delim, DelimPolicy::Include)?.unwrap()),
                    &[value, delim]
                );
                assert_eq!(
                    bytes(&reader.read_delim(delim, DelimPolicy::Exclude)?.unwrap()),
                    &[value]
                );
                assert!(reader.read_delim(delim, DelimPolicy::Exclude)?.is_none());
            });
        }
    }
    Ok(())
}

struct ShortRead<'a> {
    input: &'a [u8],
    max: usize,
}

impl Read for ShortRead<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output.len().min(self.max);
        self.input.read(&mut output[..count])
    }
}

#[test]
fn long_lines_grow_across_short_reads() -> io::Result<()> {
    let content = vec![0xff; 16_385];
    let mut input = content.clone();
    input.extend_from_slice(b"\r\ntail");
    let source = || ShortRead {
        input: &input,
        max: 7,
    };
    check_readers!(3, source(), |reader| {
        assert_eq!(
            bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
            content
        );
        assert_eq!(
            bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
            b"tail"
        );
        assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    });
    Ok(())
}

struct PauseOnce<'a> {
    prefix: &'a [u8],
    tail: &'a [u8],
    paused: bool,
}

impl Read for PauseOnce<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if !self.prefix.is_empty() {
            return self.prefix.read(output);
        }
        if !self.paused {
            self.paused = true;
            output.fill(0xee);
            return Err(ErrorKind::WouldBlock.into());
        }
        self.tail.read(output)
    }
}

#[test]
fn retrying_the_same_operation_keeps_the_prefix() -> io::Result<()> {
    for capacity in 1..=12 {
        let source = || PauseOnce {
            prefix: b"abc\r",
            tail: b"\ntail",
            paused: false,
        };
        check_readers!(capacity, source(), |reader| {
            assert_eq!(
                reader.read_line(DelimPolicy::Exclude).err().unwrap().kind(),
                ErrorKind::WouldBlock
            );
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"abc"
            );
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"tail"
            );
            assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        });
    }
    Ok(())
}

#[test]
fn changing_the_delimiter_after_an_error_reexamines_the_retained_prefix() -> io::Result<()> {
    for capacity in 1..=12 {
        let source = || PauseOnce {
            prefix: b"ab|cd\r",
            tail: b"\ntail",
            paused: false,
        };
        check_readers!(capacity, source(), |reader| {
            assert_eq!(
                reader.read_line(DelimPolicy::Exclude).err().unwrap().kind(),
                ErrorKind::WouldBlock
            );
            assert_eq!(
                bytes(&reader.read_delim(b'|', DelimPolicy::Include)?.unwrap()),
                b"ab|"
            );
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Include)?.unwrap()),
                b"cd\r\n"
            );
            let mut tail = Vec::new();
            reader.read_to_end(&mut tail)?;
            assert_eq!(tail, b"tail");
        });
    }
    Ok(())
}

#[test]
fn raw_reads_recover_the_retained_prefix_after_an_error() -> io::Result<()> {
    for capacity in 1..=12 {
        let source = || PauseOnce {
            prefix: b"abcdef",
            tail: b"ghi",
            paused: false,
        };
        check_readers!(capacity, source(), |reader| {
            assert_eq!(
                reader
                    .read_delim(b'!', DelimPolicy::Include)
                    .err()
                    .unwrap()
                    .kind(),
                ErrorKind::WouldBlock
            );
            let mut prefix = [0; 2];
            reader.read_exact(&mut prefix)?;
            assert_eq!(&prefix, b"ab");
            reader.read_exact(&mut prefix)?;
            assert_eq!(&prefix, b"cd");
            let mut tail = Vec::new();
            reader.read_to_end(&mut tail)?;
            assert_eq!(tail, b"efghi");
        });
    }
    Ok(())
}

#[test]
fn all_operations_share_one_position() -> io::Result<()> {
    for capacity in 1..=20 {
        let input = b"abc|def\r\nXYtail\n".as_slice();
        check_readers!(capacity, input, |reader| {
            assert_eq!(reader.fill_buf()?.first(), Some(&b'a'));
            reader.consume(1);
            let mut first = [0; 2];
            reader.read_exact(&mut first)?;
            assert_eq!(&first, b"bc");
            assert!(bytes(&reader.read_delim(b'|', DelimPolicy::Exclude)?.unwrap()).is_empty());
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"def"
            );
            let mut raw = [0; 2];
            reader.read_exact(&mut raw)?;
            assert_eq!(&raw, b"XY");
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"tail"
            );
            assert!(reader.fill_buf()?.is_empty());
            assert_eq!(reader.read(&mut raw)?, 0);
        });
    }
    Ok(())
}

#[test]
fn standard_bufread_methods_share_the_inherent_methods_position() -> io::Result<()> {
    for capacity in 1..=12 {
        check_readers!(capacity, b"a|b\r\nc\nd".as_slice(), |reader| {
            let mut first = Vec::new();
            assert_eq!(BufRead::read_until(&mut reader, b'|', &mut first)?, 2);
            assert_eq!(first, b"a|");
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"b"
            );
            let mut next = Vec::new();
            assert_eq!(BufRead::read_until(&mut reader, b'\n', &mut next)?, 2);
            assert_eq!(next, b"c\n");
            let mut last = [0; 1];
            reader.read_exact(&mut last)?;
            assert_eq!(&last, b"d");
            assert_eq!(reader.read(&mut last)?, 0);
        });
    }
    Ok(())
}

#[test]
fn borrowed_lines_and_delimited_segments_reuse_the_source_buffer() -> io::Result<()> {
    let input = [0xff, b'\r', b'\n', b'a', b'b', b'\n'];
    let mut reader = SliceReader::new(input.as_slice());
    let line: &[u8] = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    assert_eq!(line, &[0xff]);
    assert_eq!(line.as_ptr(), input.as_ptr());
    let segment: &[u8] = reader.read_delim(b'b', DelimPolicy::Include)?.unwrap();
    assert_eq!(segment, b"ab");
    assert_eq!(segment.as_ptr(), input[3..].as_ptr());
    assert!(reader.read_line(DelimPolicy::Exclude)?.unwrap().is_empty());
    assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    Ok(())
}

#[test]
fn dropping_a_borrowing_adapter_commits_delivered_source_bytes() -> io::Result<()> {
    let input = b"first\r\nsecond\n";
    let mut source = input.as_slice();
    {
        let mut reader = SliceReader::new(&mut source);
        assert_eq!(reader.read_line(DelimPolicy::Exclude)?.unwrap(), b"first");
    }
    assert_eq!(source, b"second\n");
    {
        let mut reader = SliceReader::new(&mut source);
        assert_eq!(
            reader.read_delim(b'c', DelimPolicy::Include)?.unwrap(),
            b"sec"
        );
    }
    assert_eq!(source, b"ond\n");
    {
        let mut reader = VecReader::new(&mut source);
        assert_eq!(reader.read_line(DelimPolicy::Include)?.unwrap(), b"ond\n");
    }
    assert!(source.is_empty());
    Ok(())
}

#[test]
fn borrowed_error_prefix_reuses_scratch_without_shifting_each_result() -> io::Result<()> {
    let source = PauseOnce {
        prefix: b"a|b|c|",
        tail: b"last",
        paused: false,
    };
    let mut reader = SliceReader::new(BufReader::with_capacity(2, source));
    assert_eq!(
        reader
            .read_delim(b'!', DelimPolicy::Include)
            .err()
            .unwrap()
            .kind(),
        ErrorKind::WouldBlock
    );
    let first = reader.read_delim(b'|', DelimPolicy::Exclude)?.unwrap();
    assert_eq!(first, b"a");
    let first_pointer = first.as_ptr();
    let second = reader.read_delim(b'|', DelimPolicy::Exclude)?.unwrap();
    assert_eq!(second, b"b");
    assert_eq!(second.as_ptr(), first_pointer.wrapping_add(2));
    let third = reader.read_delim(b'|', DelimPolicy::Exclude)?.unwrap();
    assert_eq!(third, b"c");
    assert_eq!(third.as_ptr(), first_pointer.wrapping_add(4));
    assert_eq!(reader.read_line(DelimPolicy::Exclude)?.unwrap(), b"last");
    assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    Ok(())
}

#[test]
fn owned_results_survive_subsequent_growth_and_reader_destruction() -> io::Result<()> {
    let mut input = b"first\n".to_vec();
    input.extend(std::iter::repeat_n(0xff, 16_385));
    input.extend_from_slice(b"\r\nlast");
    let mut reader = BytesReader::with_capacity(4, input.as_slice());
    let first: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    let second: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    let mut raw = [0; 4];
    reader.read_exact(&mut raw)?;
    drop(reader);
    assert_eq!(first, b"first".as_slice());
    assert_eq!(second.as_ref(), vec![0xff; 16_385]);
    assert_eq!(&raw, b"last");

    let mut reader = VecReader::new(BufReader::with_capacity(4, input.as_slice()));
    let first: Vec<u8> = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    let second: Vec<u8> = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    reader.read_exact(&mut raw)?;
    drop(reader);
    drop(input);
    assert_eq!(first, b"first");
    assert_eq!(second, vec![0xff; 16_385]);
    assert_eq!(&raw, b"last");
    Ok(())
}

#[test]
fn bytes_delivery_shares_the_current_buffer() -> io::Result<()> {
    let mut reader = BytesReader::new(b"first\nsecond\n".as_slice());
    let pointer = reader.fill_buf()?.as_ptr();
    let mut first: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    assert_eq!(first.as_ptr(), pointer);
    first[0] = b'F';
    let mut second: BytesMut = reader.read_delim(b'\n', DelimPolicy::Exclude)?.unwrap();
    assert_eq!(second.as_ptr(), pointer.wrapping_add(6));
    assert_eq!(second.as_ref(), b"second");
    drop(reader);
    second[0] = b'S';
    assert_eq!(first.as_ref(), b"First");
    assert_eq!(second.as_ref(), b"Second");
    Ok(())
}

#[test]
fn empty_reads_do_not_poll_the_source() -> io::Result<()> {
    struct MustNotRead;
    impl Read for MustNotRead {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("empty reads must not poll the source")
        }
    }
    check_readers!(4, MustNotRead, |reader| {
        assert_eq!(reader.read(&mut [])?, 0);
    });
    Ok(())
}

#[test]
fn reading_resumes_after_temporary_eof_and_delivers_each_unterminated_tail() -> io::Result<()> {
    struct ResumingSource {
        steps: VecDeque<&'static [u8]>,
    }
    impl Read for ResumingSource {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if output.is_empty() {
                return Ok(0);
            }
            let Some(input) = self.steps.front_mut() else {
                return Ok(0);
            };
            let count = input.read(output)?;
            if input.is_empty() {
                self.steps.pop_front();
            }
            Ok(count)
        }
    }
    let source = || ResumingSource {
        steps: [
            b"".as_slice(),
            b"first",
            b"",
            b"second\r\n",
            b"",
            b"raw",
            b"",
            b"last",
            b"",
            b"new\n",
        ]
        .into(),
    };
    for capacity in 1..=8 {
        check_readers!(capacity, source(), |reader| {
            assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"first"
            );
            assert_eq!(
                bytes(&reader.read_delim(b'\n', DelimPolicy::Exclude)?.unwrap()),
                b"second\r"
            );
            assert!(reader.fill_buf()?.is_empty());
            let mut raw = [0; 3];
            reader.read_exact(&mut raw)?;
            assert_eq!(&raw, b"raw");
            assert_eq!(reader.read(&mut raw)?, 0);
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Include)?.unwrap()),
                b"last"
            );
            assert_eq!(
                bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
                b"new"
            );
            assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        });
    }
    Ok(())
}

#[test]
fn interrupted_reads_retry_without_corrupting_the_prefix() -> io::Result<()> {
    struct Interrupt<'a> {
        input: &'a [u8],
        calls: usize,
    }
    impl Read for Interrupt<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            self.calls += 1;
            if self.calls % 2 == 1 {
                output.fill(0xee);
                return Err(ErrorKind::Interrupted.into());
            }
            let count = output.len().min(2);
            self.input.read(&mut output[..count])
        }
    }
    let source = || Interrupt {
        input: b"abcdef\r\n",
        calls: 0,
    };
    check_readers!(3, source(), |reader| {
        assert_eq!(
            bytes(&reader.read_line(DelimPolicy::Exclude)?.unwrap()),
            b"abcdef"
        );
        assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    });
    Ok(())
}

#[test]
fn bytes_reader_keeps_the_fill_size_after_long_lines_and_short_reads() -> io::Result<()> {
    struct ObservedRead<'a> {
        input: &'a [u8],
        requested: &'a RefCell<Vec<usize>>,
    }
    impl Read for ObservedRead<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            self.requested.borrow_mut().push(output.len());
            let count = output.len().min(3);
            self.input.read(&mut output[..count])
        }
    }
    let requested = RefCell::new(Vec::new());
    let source = ObservedRead {
        input: b"abcdefghijkl\nz\n",
        requested: &requested,
    };
    let mut reader = BytesReader::with_capacity(4, source);
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude)?.unwrap().as_ref(),
        b"abcdefghijkl"
    );
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude)?.unwrap(),
        b"z".as_slice()
    );
    assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    assert!(!requested.borrow().is_empty());
    assert!(requested.borrow().iter().all(|&size| size == 4));
    Ok(())
}

#[test]
fn bytes_reader_consumes_only_the_ending_of_a_spanning_line() -> io::Result<()> {
    let mut reader = BytesReader::with_capacity(8, b"1234567\r\nnext\n".as_slice());
    let first = reader.read_line(DelimPolicy::Exclude)?.unwrap();
    assert_eq!(first.as_ref(), b"1234567");
    assert_eq!(reader.fill_buf()?, b"next\n");
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude)?.unwrap().as_ref(),
        b"next"
    );
    assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    Ok(())
}

#[test]
fn bytes_reader_returns_buffered_input_without_polling_the_source() -> io::Result<()> {
    struct ReadOnce(bool);
    impl Read for ReadOnce {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            assert!(!self.0, "unconsumed input must not poll the source");
            self.0 = true;
            b"abc\n".as_slice().read(output)
        }
    }
    let mut reader = BytesReader::with_capacity(4, ReadOnce(false));
    assert_eq!(reader.fill_buf()?, b"abc\n");
    assert_eq!(reader.fill_buf()?, b"abc\n");
    reader.consume(1);
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude)?.unwrap(),
        b"bc".as_slice()
    );
    Ok(())
}

#[test]
fn bytes_reader_discards_bytes_written_by_a_failed_fill() -> io::Result<()> {
    let source = PauseOnce {
        prefix: b"",
        tail: b"ok",
        paused: false,
    };
    let mut reader = BytesReader::with_capacity(4, source);
    assert_eq!(reader.fill_buf().unwrap_err().kind(), ErrorKind::WouldBlock);
    assert_eq!(reader.fill_buf()?, b"ok");
    reader.consume(2);
    assert!(reader.fill_buf()?.is_empty());
    Ok(())
}

#[test]
fn bytes_reader_can_retry_after_an_error_before_any_input() -> io::Result<()> {
    for capacity in 1..=8 {
        let source = PauseOnce {
            prefix: b"",
            tail: b"\r\nnext",
            paused: false,
        };
        let mut reader = BytesReader::with_capacity(capacity, source);
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        assert!(reader.read_line(DelimPolicy::Exclude)?.unwrap().is_empty());
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude)?.unwrap().as_ref(),
            b"next"
        );
        assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    }
    Ok(())
}

#[test]
fn bytes_reader_releases_spanning_prefix_during_unwind() -> io::Result<()> {
    struct PanicAfterInput(&'static [u8]);
    impl Read for PanicAfterInput {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            assert!(!self.0.is_empty(), "source panicked after delivering input");
            self.0.read(output)
        }
    }

    let input = b"held\npartial";
    let mut reader = BytesReader::with_capacity(input.len(), PanicAfterInput(input));
    let mut held = reader.read_line(DelimPolicy::Include)?.unwrap();
    let additional = input.len() - held.len();
    // The reader still owns a view into the same allocation.
    assert!(!held.try_reclaim(additional));

    let result = catch_unwind(AssertUnwindSafe(|| reader.read_line(DelimPolicy::Exclude)));
    assert!(result.is_err());
    // Unwinding must drop the accumulated prefix, allowing the held result
    // to reclaim the allocation. A leaked prefix would keep it shared.
    assert!(held.try_reclaim(additional));
    assert_eq!(held.as_ref(), b"held\n");
    Ok(())
}

#[test]
fn bytes_reader_shares_error_prefix_with_bufread_and_changed_delimiters() -> io::Result<()> {
    let source = PauseOnce {
        prefix: b"ab|cd\r",
        tail: b"\nend",
        paused: false,
    };
    let mut reader = BytesReader::with_capacity(8, source);
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert_eq!(reader.fill_buf()?, b"ab|cd\r");
    reader.consume(2);
    assert!(
        reader
            .read_delim(b'|', DelimPolicy::Exclude)?
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude)?.unwrap(),
        b"cd".as_slice()
    );
    let mut tail = Vec::new();
    reader.read_to_end(&mut tail)?;
    assert_eq!(tail, b"end");
    Ok(())
}

#[test]
fn bytes_reader_grows_from_zero_capacity_before_reading() -> io::Result<()> {
    let mut reader = BytesReader::with_capacity(0, b"\xff\r\nraw|last".as_slice());
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude)?.unwrap().as_ref(),
        b"\xff"
    );
    let mut raw = [0; 3];
    reader.read_exact(&mut raw)?;
    assert_eq!(&raw, b"raw");
    assert!(
        reader
            .read_delim(b'|', DelimPolicy::Exclude)?
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        reader.read_line(DelimPolicy::Exclude)?.unwrap().as_ref(),
        b"last"
    );
    assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());

    let mut reader = BytesReader::with_capacity(0, b"raw".as_slice());
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    assert_eq!(bytes, b"raw");
    Ok(())
}

#[test]
fn readers_preserve_send_and_sync_for_capable_sources() {
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    assert_send_sync(&VecReader::new(b"input".as_slice()));
    assert_send_sync(&SliceReader::new(b"input".as_slice()));
    assert_send_sync(&BytesReader::new(b"input".as_slice()));
}

#[test]
fn a_slice_reborrow_error_does_not_consume_the_selected_bytes() -> io::Result<()> {
    struct ReborrowInterrupt<'a> {
        input: &'a [u8],
        calls: usize,
    }
    impl Read for ReborrowInterrupt<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            self.input.read(output)
        }
    }
    impl BufRead for ReborrowInterrupt<'_> {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            self.calls += 1;
            if self.calls == 2 {
                return Err(ErrorKind::Interrupted.into());
            }
            Ok(self.input)
        }

        fn consume(&mut self, count: usize) {
            self.input = &self.input[count..];
        }
    }
    let mut reader = SliceReader::new(ReborrowInterrupt {
        input: b"first\nsecond\n",
        calls: 0,
    });
    match reader.read_line(DelimPolicy::Exclude) {
        Err(error) => {
            assert_eq!(error.kind(), ErrorKind::Interrupted);
            assert_eq!(reader.read_line(DelimPolicy::Exclude)?.unwrap(), b"first");
        }
        Ok(line) => assert_eq!(line.unwrap(), b"first"),
    }
    assert_eq!(reader.read_line(DelimPolicy::Exclude)?.unwrap(), b"second");
    assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
    Ok(())
}
