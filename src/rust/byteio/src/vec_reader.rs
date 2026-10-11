use std::io::{self, BufRead, ErrorKind, Read};

use memchr::memchr;

use crate::DelimPolicy;

/// Read independently owned byte segments from an existing buffered source.
///
/// Delimited segments, lines, [`Read`] and [`BufRead`] share one input position.
/// Results remain valid across subsequent reads and after the reader is dropped. No
/// bytes are decoded as text; empty lines are `Some(empty)` and EOF is `None`.
/// Lines have no fixed length limit. Later calls check the source again after EOF.
///
/// Keep this reader after an I/O error to recover undelivered bytes. Dropping it
/// discards any retained, undelivered prefix. Memory exhaustion follows the
/// standard allocator's behavior and is not guaranteed to return an I/O error.
#[derive(Debug)]
pub struct VecReader<R: BufRead> {
    source: R,
    // Copy only when a segment spans source buffers. Successful owned delivery
    // can transfer this allocation; errors keep the incomplete prefix here.
    prefix: Vec<u8>,
    // Raw reads and a delimiter change can consume only part of an error prefix.
    start: usize,
}

impl<R: BufRead> VecReader<R> {
    /// Start reading from the buffered source's current position.
    ///
    /// Pass the source by value to transfer ownership, or use `&mut source` to
    /// borrow it. No additional input buffer is created.
    pub fn new(source: R) -> Self {
        Self {
            source,
            prefix: Vec::new(),
            start: 0,
        }
    }

    /// Read through the first matching byte, or the remaining input at EOF.
    ///
    /// Both policies consume the delimiter. `Exclude` removes only the matching
    /// byte; excluding LF here preserves a preceding CR. Adjacent delimiters
    /// produce empty results, whereas EOF is `None`.
    ///
    /// # Errors
    /// Source errors retain the entire undelivered prefix for a later call,
    /// including when the delimiter or reading method changes. Interrupted
    /// source fills are retried.
    pub fn read_delim(
        &mut self,
        delim: u8,
        delim_policy: DelimPolicy,
    ) -> io::Result<Option<Vec<u8>>> {
        self.read_delimited(delim, delim_policy, false)
    }

    /// Read a byte line terminated by LF, or the unterminated tail at EOF.
    ///
    /// `Exclude` removes LF and one immediately preceding CR; isolated CR and
    /// all other bytes are preserved. Empty lines are `Some(empty)`. EOF is
    /// `None`, and a final delimiter does not create an additional empty line.
    ///
    /// # Errors
    /// Follows [`Self::read_delim`], retaining undelivered bytes on source errors
    /// and retrying interrupted fills.
    pub fn read_line(&mut self, delim_policy: DelimPolicy) -> io::Result<Option<Vec<u8>>> {
        self.read_delimited(b'\n', delim_policy, true)
    }

    fn read_delimited(
        &mut self,
        delim: u8,
        delim_policy: DelimPolicy,
        strip_cr: bool,
    ) -> io::Result<Option<Vec<u8>>> {
        if let Some(at) = memchr(delim, &self.prefix[self.start..]) {
            let end = self.start + at + 1;
            let visible = delim_policy.delim_end(&self.prefix[self.start..end], strip_cr);
            return Ok(Some(self.take_prefix(end, visible)));
        }
        if self.start != 0 {
            // Compact only before appending to the last incomplete suffix.
            // Repeated small deliveries from an error prefix need not move it.
            self.prefix.copy_within(self.start.., 0);
            self.prefix.truncate(self.prefix.len() - self.start);
            self.start = 0;
        }
        loop {
            let input = match self.source.fill_buf() {
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                result => result?,
            };
            if input.is_empty() {
                break;
            }
            if let Some(at) = memchr(delim, input) {
                let count = at + 1;
                if self.prefix.is_empty() {
                    // The source already holds the whole segment: copy only
                    // its delivered content straight into the returned Vec.
                    let visible = delim_policy.delim_end(&input[..count], strip_cr);
                    let output = input[..visible].to_vec();
                    self.source.consume(count);
                    return Ok(Some(output));
                }
                self.prefix.extend_from_slice(&input[..count]);
                self.source.consume(count);
                let visible = delim_policy.delim_end(&self.prefix, strip_cr);
                return Ok(Some(self.take_prefix(self.prefix.len(), visible)));
            }
            let count = input.len();
            self.prefix.extend_from_slice(input);
            self.source.consume(count);
        }
        if self.prefix.is_empty() {
            return Ok(None);
        }
        let end = self.prefix.len();
        Ok(Some(self.take_prefix(end, end)))
    }

    fn take_prefix(&mut self, end: usize, visible: usize) -> Vec<u8> {
        if end == self.prefix.len() {
            if self.start != 0 {
                self.prefix.copy_within(self.start..end, 0);
            }
            let mut output = std::mem::take(&mut self.prefix);
            output.truncate(visible);
            self.start = 0;
            output
        } else {
            let output = self.prefix[self.start..self.start + visible].to_vec();
            self.start = end;
            output
        }
    }
}

impl<R: BufRead> Read for VecReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let input = match self.fill_buf() {
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                result => result?,
            };
            let count = output.len().min(input.len());
            output[..count].copy_from_slice(&input[..count]);
            self.consume(count);
            return Ok(count);
        }
    }
}

impl<R: BufRead> BufRead for VecReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.start < self.prefix.len() {
            return Ok(&self.prefix[self.start..]);
        }
        self.source.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        if self.start < self.prefix.len() {
            self.start += amount.min(self.prefix.len() - self.start);
            if self.start == self.prefix.len() {
                self.prefix.clear();
                self.start = 0;
            }
        } else {
            self.source.consume(amount);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        io::{self, BufRead, BufReader, ErrorKind, Read},
    };

    use super::VecReader;
    use crate::DelimPolicy;

    enum Event {
        Bytes(&'static [u8]),
        Error(ErrorKind),
        Eof,
    }

    struct Scripted {
        events: VecDeque<Event>,
        input: &'static [u8],
        fills: usize,
    }

    impl Scripted {
        fn new(events: impl IntoIterator<Item = Event>) -> Self {
            Self {
                events: events.into_iter().collect(),
                input: &[],
                fills: 0,
            }
        }
    }

    impl BufRead for Scripted {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            self.fills += 1;
            if self.input.is_empty() {
                match self.events.pop_front() {
                    Some(Event::Bytes(input)) => self.input = input,
                    Some(Event::Error(kind)) => return Err(kind.into()),
                    Some(Event::Eof) | None => {}
                }
            }
            Ok(self.input)
        }

        fn consume(&mut self, count: usize) {
            self.input = &self.input[count.min(self.input.len())..];
        }
    }

    impl Read for Scripted {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if output.is_empty() {
                return Ok(0);
            }
            let input = self.fill_buf()?;
            let count = output.len().min(input.len());
            output[..count].copy_from_slice(&input[..count]);
            self.consume(count);
            Ok(count)
        }
    }

    #[test]
    fn complete_buffered_lines_need_no_auxiliary_buffer() -> io::Result<()> {
        let input = b"one\r\ntwo\n";
        let mut reader = VecReader::new(input.as_slice());
        let first = reader.read_line(DelimPolicy::Exclude)?.unwrap();
        let second = reader.read_line(DelimPolicy::Include)?.unwrap();
        assert_eq!(reader.prefix.capacity(), 0);
        drop(reader);
        assert_eq!(first, b"one");
        assert_eq!(second, b"two\n");
        assert_ne!(first.as_ptr(), input.as_ptr());
        Ok(())
    }

    #[test]
    fn all_buffer_boundaries_preserve_byte_line_contract() -> io::Result<()> {
        let input = b"\n\r\n\r\r\n\0\xff\r\ntail\r";
        let expected = [b"".as_slice(), b"", b"\r", b"\0\xff", b"tail\r"];
        for capacity in 1..=input.len() {
            let mut reader = VecReader::new(BufReader::with_capacity(capacity, input.as_slice()));
            for value in expected {
                assert_eq!(
                    reader.read_line(DelimPolicy::Exclude)?,
                    Some(value.to_vec())
                );
            }
            assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        }
        Ok(())
    }

    #[test]
    fn failed_prefix_survives_raw_reads_and_delimiter_changes() -> io::Result<()> {
        let source = Scripted::new([
            Event::Bytes(b"ab|cd\nef"),
            Event::Error(ErrorKind::WouldBlock),
            Event::Bytes(b"gh\n"),
        ]);
        let mut reader = VecReader::new(source);
        assert_eq!(
            reader
                .read_delim(b'!', DelimPolicy::Exclude)
                .unwrap_err()
                .kind(),
            ErrorKind::WouldBlock
        );
        let mut raw = [0; 2];
        reader.read_exact(&mut raw)?;
        assert_eq!(&raw, b"ab");
        assert_eq!(reader.read_delim(b'|', DelimPolicy::Exclude)?, Some(vec![]));
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude)?,
            Some(b"cd".to_vec())
        );
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude)?,
            Some(b"efgh".to_vec())
        );
        assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        Ok(())
    }

    #[test]
    fn bufread_and_read_share_an_incomplete_line_after_error() -> io::Result<()> {
        let source = Scripted::new([
            Event::Bytes(b"abc\r"),
            Event::Error(ErrorKind::Interrupted),
            Event::Error(ErrorKind::WouldBlock),
            Event::Bytes(b"\ndef"),
        ]);
        let mut reader = VecReader::new(source);
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude).unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
        assert_eq!(reader.fill_buf()?, b"abc\r");
        reader.consume(1);
        let mut raw = [0; 2];
        reader.read_exact(&mut raw)?;
        assert_eq!(&raw, b"bc");
        assert_eq!(reader.read_line(DelimPolicy::Exclude)?, Some(vec![]));
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude)?,
            Some(b"def".to_vec())
        );
        Ok(())
    }

    #[test]
    fn completed_spanning_line_transfers_its_prefix_allocation() -> io::Result<()> {
        let source = Scripted::new([
            Event::Bytes(b"abcdef"),
            Event::Error(ErrorKind::WouldBlock),
            Event::Bytes(b"\nrest"),
        ]);
        let mut reader = VecReader::new(source);
        assert!(reader.read_line(DelimPolicy::Exclude).is_err());
        reader.prefix.reserve(1);
        let pointer = reader.prefix.as_ptr();
        let line = reader.read_line(DelimPolicy::Exclude)?.unwrap();
        assert_eq!(line, b"abcdef");
        assert_eq!(line.as_ptr(), pointer);
        let mut raw = [0; 4];
        reader.read_exact(&mut raw)?;
        assert_eq!(&raw, b"rest");
        Ok(())
    }

    #[test]
    fn empty_raw_reads_do_not_poll_input() -> io::Result<()> {
        let mut reader = VecReader::new(Scripted::new([Event::Bytes(b"a\n")]));
        assert_eq!(reader.read(&mut [])?, 0);
        assert_eq!(reader.source.fills, 0);
        assert_eq!(reader.read_line(DelimPolicy::Exclude)?, Some(b"a".to_vec()));
        Ok(())
    }

    #[test]
    fn later_calls_read_new_data_after_temporary_eof() -> io::Result<()> {
        let mut reader = VecReader::new(Scripted::new([
            Event::Bytes(b"tail\r"),
            Event::Eof,
            Event::Bytes(b"new\n"),
            Event::Eof,
            Event::Bytes(b"raw"),
            Event::Eof,
            Event::Bytes(b"more\n"),
        ]));
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude)?,
            Some(b"tail\r".to_vec())
        );
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude)?,
            Some(b"new".to_vec())
        );
        assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        let mut raw = [0; 3];
        reader.read_exact(&mut raw)?;
        assert_eq!(&raw, b"raw");
        assert_eq!(reader.read(&mut raw)?, 0);
        assert_eq!(reader.fill_buf()?, b"more\n");
        reader.consume(5);
        assert!(reader.read_line(DelimPolicy::Exclude)?.is_none());
        Ok(())
    }
}
