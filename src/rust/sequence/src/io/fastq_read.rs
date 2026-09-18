use std::io::Read;

use thiserror::Error;
#[derive(Debug, Error)]
pub(crate) enum FastqReadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Parse(#[from] FastqParseError),
}
type Result<T> = std::result::Result<T, FastqReadError>;
use bytes::Bytes;
use memchr::memchr2;

use super::fastq_error::FastqParseError;
use crate::FastqRecord;
use mire_streaming::LineReader;

pub(crate) struct FastqReader<R> {
    reader: LineReader<R>,
}

impl<R: Read> FastqReader<R> {
    #[allow(dead_code)]
    pub(crate) fn new(reader: R) -> Self {
        Self::with_capacity(8 * 1024, reader)
    }

    pub(crate) fn with_capacity(capacity: usize, reader: R) -> Self {
        Self {
            reader: LineReader::with_capacity(capacity, reader),
        }
    }

    pub(crate) fn offset(&self) -> usize {
        self.reader.offset()
    }

    #[inline]
    fn read_line(&mut self) -> std::io::Result<Option<Bytes>> {
        self.reader.read_line()
    }

    pub(crate) fn read_record(&mut self) -> Result<Option<FastqRecord<Bytes>>> {
        let Some(header) = self.read_header()? else {
            return Ok(None);
        };
        let (id, desc) = parse_header(&header, self.offset())?;
        let seq = self.required_line(|| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&id),
                String::from_utf8_lossy(desc.as_deref().unwrap_or_default())
            )
        })?;
        let sep = self.required_line(|| display_lines(&[&header, &seq]))?;
        validate_separator(&header, &seq, &sep, self.offset())?;
        let qual = self
            .required_line(|| display_lines(&[desc.as_deref().unwrap_or_default(), &seq, &sep]))?;
        validate_quality(&header, &seq, &sep, &qual, self.offset())?;
        Ok(Some(FastqRecord::new(id, desc, seq, sep, qual)))
    }

    fn read_header(&mut self) -> Result<Option<Bytes>> {
        while let Some(line) = self.read_line()? {
            if !line.iter().all(u8::is_ascii_whitespace) {
                return Ok(Some(line));
            }
        }
        Ok(None)
    }

    fn required_line(&mut self, context: impl FnOnce() -> String) -> Result<Bytes> {
        self.read_line()?.ok_or_else(|| {
            FastqParseError::IncompleteRecord {
                record: context(),
                pos: self.offset(),
            }
            .into()
        })
    }
}

fn parse_header(header: &Bytes, pos: usize) -> Result<(Bytes, Option<Bytes>)> {
    if header.first() != Some(&b'@') {
        return Err(FastqParseError::InvalidHead {
            record: String::from_utf8_lossy(header).into_owned(),
            pos,
        }
        .into());
    }
    Ok(if let Some(separator) = memchr2(b' ', b'\t', header) {
        (
            header.slice(1..separator),
            (separator + 1 < header.len()).then(|| header.slice(separator + 1..)),
        )
    } else {
        (header.slice(1..), None)
    })
}

fn display_lines(lines: &[&[u8]]) -> String {
    lines
        .iter()
        .map(|line| String::from_utf8_lossy(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn validate_separator(header: &[u8], seq: &[u8], sep: &[u8], pos: usize) -> Result<()> {
    if sep.first() != Some(&b'+') {
        return Err(FastqParseError::InvalidSep {
            record: display_lines(&[header, seq, sep]),
            pos,
        }
        .into());
    }
    Ok(())
}

fn validate_quality(header: &[u8], seq: &[u8], sep: &[u8], qual: &[u8], pos: usize) -> Result<()> {
    if seq.len() != qual.len() {
        return Err(FastqParseError::UnequalLength {
            seq: seq.len(),
            qual: qual.len(),
            record: display_lines(&[header, seq, sep, qual]),
            pos,
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn preserves_short_descriptions_and_accepts_empty_descriptions() {
        for (header, expected) in [
            (b"@read x".as_slice(), Some(b"x".as_slice())),
            (b"@read\tx".as_slice(), Some(b"x".as_slice())),
            (b"@read ".as_slice(), None),
            (b"@read".as_slice(), None),
        ] {
            let input = [header, b"\nAC\n+\n12\n"].concat();
            let mut reader = FastqReader::new(Cursor::new(input));
            let record = reader.read_record().unwrap().unwrap();
            assert_eq!(record.id.as_ref(), b"read");
            assert_eq!(record.desc.as_deref(), expected);
        }
    }

    fn create_reader(data: &str) -> FastqReader<Cursor<&[u8]>> {
        let reader = Cursor::new(data.as_bytes());
        FastqReader::new(reader)
    }

    #[test]
    fn test_read_valid_record() -> Result<()> {
        let fastq_data = "@seq1 description\nATGC\n+\n!!!!\n@seq2\nGCGT\n+\n$$$$\n";

        let mut reader = create_reader(fastq_data);

        let record = reader.read_record()?.expect("Should have a record");

        // Check that the FASTQ fields match the expected values
        assert_eq!(record.id.as_ref(), b"seq1");
        assert_eq!(record.desc.unwrap().as_ref(), b"description");
        assert_eq!(record.seq.as_ref(), b"ATGC");
        assert_eq!(record.sep.as_ref(), b"+");
        assert_eq!(record.qual.as_ref(), b"!!!!");

        Ok(())
    }

    #[test]
    fn test_invalid_header() -> Result<()> {
        let fastq_data = "seq1 description\nATGC\n+\n!!!!\n";

        let mut reader = create_reader(fastq_data);

        let result = reader.read_record();

        assert!(result.is_err()); // Expect error: Invalid header
        Ok(())
    }

    #[test]
    fn test_incomplete_record() -> Result<()> {
        let fastq_data = "@seq1 description\nATGC\n+\n";

        let mut reader = create_reader(fastq_data);

        let result = reader.read_record();

        assert!(result.is_err()); // Expect error: Incomplete record (missing quality)
        Ok(())
    }

    #[test]
    fn test_unmatched_seq_qual_length() -> Result<()> {
        let fastq_data = "@seq1 description\nATGC\n+\n!!\n";

        let mut reader = create_reader(fastq_data);

        let result = reader.read_record();

        assert!(result.is_err()); // Expect error: Unequal sequence and quality lengths
        Ok(())
    }

    #[test]
    fn test_edge_case_empty_data() -> Result<()> {
        let fastq_data = "";

        let mut reader = create_reader(fastq_data);

        let result = reader.read_record();

        assert!(result.is_ok()); // Expect error: EOF
        assert!(result.unwrap().is_none());
        Ok(())
    }
}
