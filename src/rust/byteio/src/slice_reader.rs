use std::io::{self, BufRead, ErrorKind, Read};

use memchr::memchr;

use crate::DelimPolicy;

/// Borrow byte segments from an existing buffered source.
///
/// Results borrow this reader and must finish being used before another mutable
/// operation. Delimited reads, [`Read`] and [`BufRead`] share one input position.
/// Empty segments are `Some(empty)`; EOF is `None`. Later calls can read input
/// that becomes available after EOF.
///
/// Keep the reader after an error to recover undelivered input. Dropping it
/// discards any retained partial segment, while committing a successfully
/// delivered segment to a borrowed source.
///
/// ```
/// use mire_byteio::{SliceReader, DelimPolicy};
/// let mut reader = SliceReader::new(b"a\r\n\xff".as_slice());
/// assert_eq!(reader.read_line(DelimPolicy::Exclude)?, Some(b"a".as_slice()));
/// assert_eq!(reader.read_line(DelimPolicy::Exclude)?, Some(b"\xff".as_slice()));
/// # Ok::<(), std::io::Error>(())
/// ```
///
/// ```compile_fail
/// use mire_byteio::{SliceReader, DelimPolicy};
/// let mut reader = SliceReader::new(b"a\nb\n".as_slice());
/// let first = reader.read_line(DelimPolicy::Exclude).unwrap().unwrap();
/// let second = reader.read_line(DelimPolicy::Exclude).unwrap();
/// assert_eq!(first, b"a");
/// ```
#[derive(Debug)]
pub struct SliceReader<R: BufRead> {
    source: R,
    // Only segments that need another source buffer are assembled here. A
    // retained error prefix can contain multiple segments after changing the
    // delimiter; start advances without moving that remainder on every read.
    scratch: Vec<u8>,
    start: usize,
    // Directly borrowed input is logically delivered, but cannot be physically
    // consumed until the returned borrow ends. Drop commits this prefix too.
    pending: usize,
}

enum Location {
    Input(usize),
    Scratch(usize),
    Eof,
}

impl<R: BufRead> SliceReader<R> {
    /// Start borrowing byte segments at the source's current input position.
    pub fn new(source: R) -> Self {
        Self {
            source,
            scratch: Vec::new(),
            start: 0,
            pending: 0,
        }
    }

    /// Borrow through the first matching byte, or the remaining input at EOF.
    ///
    /// Both policies consume the delimiter; `Exclude` removes only that byte.
    /// In particular, excluding LF leaves a preceding CR. Adjacent delimiters
    /// produce empty segments. An unterminated final segment is unchanged.
    ///
    /// # Errors
    /// Interrupted refills are retried. An error from the final reborrow is
    /// propagated without consumption, including `Interrupted` from a custom
    /// source on an unconsumed buffer. A source that shrinks that buffer
    /// unexpectedly returns [`ErrorKind::InvalidData`].
    ///
    /// Already read, undelivered prefixes remain available after errors. The
    /// delimiter and policy may change on the next call, or raw reading may drain
    /// the prefix.
    pub fn read_delim(
        &mut self,
        delim: u8,
        delim_policy: DelimPolicy,
    ) -> io::Result<Option<&[u8]>> {
        self.delimited(delim, delim_policy, false)
    }

    /// Borrow a byte line ending in LF or CRLF, or the unterminated final line.
    ///
    /// `Exclude` removes LF and one immediately preceding CR. Isolated CR and
    /// all other bytes are unchanged. Empty lines are `Some(empty)`; a final
    /// delimiter does not create an additional empty line.
    ///
    /// # Errors
    /// Follows [`Self::read_delim`], preserving undelivered prefixes for retry.
    pub fn read_line(&mut self, delim_policy: DelimPolicy) -> io::Result<Option<&[u8]>> {
        self.delimited(b'\n', delim_policy, true)
    }

    fn delimited(
        &mut self,
        delim: u8,
        delim_policy: DelimPolicy,
        strip_cr: bool,
    ) -> io::Result<Option<&[u8]>> {
        match self.prepare_until(delim)? {
            Location::Input(count) => {
                // Locating the result separately avoids retaining a source
                // borrow through a refill loop. Standard sources do not refill
                // this unconsumed buffer.
                let input = self.source.fill_buf()?;
                let raw = input.get(..count).ok_or(ErrorKind::InvalidData)?;
                self.pending = count;
                Ok(Some(&raw[..delim_policy.delim_end(raw, strip_cr)]))
            }
            Location::Scratch(count) => {
                let start = self.start;
                self.start = count;
                let raw = &self.scratch[start..count];
                let end = if raw.last() == Some(&delim) {
                    delim_policy.delim_end(raw, strip_cr)
                } else {
                    raw.len()
                };
                Ok(Some(&raw[..end]))
            }
            Location::Eof => Ok(None),
        }
    }

    fn settle(&mut self) {
        if self.pending != 0 {
            self.source.consume(std::mem::take(&mut self.pending));
        }
        if self.start == self.scratch.len() {
            self.scratch.clear();
            self.start = 0;
        }
    }

    fn prepare_until(&mut self, delim: u8) -> io::Result<Location> {
        self.settle();
        if let Some(at) = memchr(delim, &self.scratch[self.start..]) {
            return Ok(Location::Scratch(self.start + at + 1));
        }
        if self.start != 0 {
            // Compact only an unfinished suffix before appending more input.
            // Delivering many retained segments never shifts the whole suffix
            // once per segment.
            self.scratch.copy_within(self.start.., 0);
            self.scratch.truncate(self.scratch.len() - self.start);
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
            let boundary = memchr(delim, input);
            let count = boundary.map_or(input.len(), |at| at + 1);
            if boundary.is_some() && self.scratch.is_empty() {
                return Ok(Location::Input(count));
            }
            self.scratch.extend_from_slice(&input[..count]);
            self.source.consume(count);
            if boundary.is_some() {
                return Ok(Location::Scratch(self.scratch.len()));
            }
        }
        Ok(if self.scratch.is_empty() {
            Location::Eof
        } else {
            Location::Scratch(self.scratch.len())
        })
    }
}

impl<R: BufRead> Read for SliceReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let count = loop {
            match self.fill_buf() {
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                result => {
                    let input = result?;
                    let count = output.len().min(input.len());
                    output[..count].copy_from_slice(&input[..count]);
                    break count;
                }
            }
        };
        self.consume(count);
        Ok(count)
    }
}

impl<R: BufRead> BufRead for SliceReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.settle();
        if self.start < self.scratch.len() {
            return Ok(&self.scratch[self.start..]);
        }
        self.source.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.settle();
        if self.start < self.scratch.len() {
            self.start += amount.min(self.scratch.len() - self.start);
        } else {
            self.source.consume(amount);
        }
    }
}

impl<R: BufRead> Drop for SliceReader<R> {
    fn drop(&mut self) {
        if self.pending != 0 {
            self.source.consume(self.pending);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, BufRead, BufReader, ErrorKind, Read};

    use super::SliceReader;
    use crate::DelimPolicy;

    #[test]
    fn terminated_lines_borrow_the_existing_input_without_scratch() -> io::Result<()> {
        let input = b"\xff\0\r\n\nnext\n";
        let mut reader = SliceReader::new(input.as_slice());
        let first = reader.read_line(DelimPolicy::Exclude)?.unwrap();
        assert_eq!(first, b"\xff\0");
        assert_eq!(first.as_ptr(), input.as_ptr());
        assert_eq!(reader.scratch.capacity(), 0);
        assert!(reader.read_line(DelimPolicy::Exclude)?.unwrap().is_empty());
        let last = reader.read_line(DelimPolicy::Exclude)?.unwrap();
        assert_eq!(last, b"next");
        assert_eq!(last.as_ptr(), input[5..].as_ptr());
        assert_eq!(reader.scratch.capacity(), 0);
        Ok(())
    }

    #[test]
    fn spanning_lines_reuse_scratch_and_then_return_to_direct_borrowing() -> io::Result<()> {
        let mut reader = SliceReader::new(BufReader::with_capacity(
            2,
            b"abcdef\nghijkl\nx\n".as_slice(),
        ));
        let first = reader.read_line(DelimPolicy::Exclude)?.unwrap();
        assert_eq!(first, b"abcdef");
        let pointer = first.as_ptr();
        let capacity = reader.scratch.capacity();
        let second = reader.read_line(DelimPolicy::Exclude)?.unwrap();
        assert_eq!(second, b"ghijkl");
        assert_eq!(second.as_ptr(), pointer);
        assert_eq!(reader.scratch.capacity(), capacity);
        let direct_pointer = reader.fill_buf()?.as_ptr();
        let third = reader.read_line(DelimPolicy::Exclude)?.unwrap();
        assert_eq!(third, b"x");
        assert_eq!(third.as_ptr(), direct_pointer);
        assert!(reader.scratch.is_empty());
        Ok(())
    }

    #[test]
    fn dropping_commits_only_the_successfully_delivered_source_prefix() -> io::Result<()> {
        let mut source = b"abc\r\nnext\n".as_slice();
        {
            let mut reader = SliceReader::new(&mut source);
            assert_eq!(
                reader.read_line(DelimPolicy::Exclude)?,
                Some(b"abc".as_slice())
            );
        }
        assert_eq!(source, b"next\n");
        {
            let mut reader = SliceReader::new(&mut source);
            assert_eq!(
                reader.read_delim(b'e', DelimPolicy::Include)?,
                Some(b"ne".as_slice())
            );
        }
        assert_eq!(source, b"xt\n");
        Ok(())
    }

    #[test]
    fn a_final_reborrow_error_does_not_mark_input_as_delivered() -> io::Result<()> {
        #[derive(Debug)]
        struct InterruptedReborrow<'a> {
            input: &'a [u8],
            calls: usize,
        }
        impl Read for InterruptedReborrow<'_> {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                self.input.read(output)
            }
        }
        impl BufRead for InterruptedReborrow<'_> {
            fn fill_buf(&mut self) -> io::Result<&[u8]> {
                self.calls += 1;
                if self.calls == 2 {
                    return Err(ErrorKind::Interrupted.into());
                }
                Ok(self.input)
            }
            fn consume(&mut self, amount: usize) {
                self.input = &self.input[amount..];
            }
        }
        let mut source = InterruptedReborrow {
            input: b"abc\nnext",
            calls: 0,
        };
        {
            let mut reader = SliceReader::new(&mut source);
            let error = reader.read_line(DelimPolicy::Exclude).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Interrupted);
        }
        assert_eq!(source.input, b"abc\nnext");
        let mut reader = SliceReader::new(&mut source);
        assert_eq!(
            reader.read_line(DelimPolicy::Exclude)?,
            Some(b"abc".as_slice())
        );
        Ok(())
    }
}
