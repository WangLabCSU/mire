use std::error::Error;
use std::fmt;

/// FASTQ parsing error
#[derive(Debug)]
pub enum FastqParseError {
    InvalidHead {
        record: String,
        pos: usize,
    },
    UnequalLength {
        record: String,
        seq: usize,
        qual: usize,
        pos: usize,
    },
    InvalidSep {
        record: String,
        pos: usize,
    },
    IncompleteRecord {
        record: String,
        pos: usize,
    },
}

impl fmt::Display for FastqParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FastqParseError::IncompleteRecord { record, pos } => {
                write!(
                    f,
                    "FASTQ parse error (line: {}): incomplete record\n{}",
                    pos, record
                )
            }
            FastqParseError::InvalidHead { record, pos } => {
                write!(
                    f,
                    "FASTQ parse error (line: {}): expected '@' at record start\n{}",
                    pos, record
                )
            }
            FastqParseError::UnequalLength {
                record,
                seq,
                qual,
                pos,
            } => {
                write!(
                    f,
                    "FASTQ parse error (line: {}): sequence and quality lengths do not match ({} vs {})\n{}",
                    pos,
                    seq, qual,
                    record
                )
            }
            FastqParseError::InvalidSep { record, pos } => {
                write!(
                    f,
                    "FASTQ parse error (line: {}): expected '+' at separator line start\n{}",
                    pos, record
                )
            }
        }
    }
}

impl Error for FastqParseError {}

#[cfg(test)]
mod test_error {
    use super::*;

    #[test]
    fn test_invalid_head() {
        let err = FastqParseError::InvalidHead {
            record: "head: @SEQ_ID".into(),
            pos: 42,
        };
        let msg = format!("{}", err);
        assert!(
            msg.contains("(line: 42)"),
            "Message should contain label and line"
        );
        assert!(
            msg.contains("expected '@' at record start"),
            "Message should mention invalid head"
        );
    }

    #[test]
    fn test_unequal_length_without_label() {
        let err = FastqParseError::UnequalLength {
            record: "SEQ\nQUAL".into(),
            seq: 10,
            qual: 8,
            pos: 100,
        };
        let msg = format!("{}", err);
        assert!(
            msg.contains("(line: 100)"),
            "Should fall back to just line number"
        );
        assert!(
            msg.contains("sequence and quality lengths do not match (10 vs 8)"),
            "Should indicate length mismatch"
        );
    }
}
