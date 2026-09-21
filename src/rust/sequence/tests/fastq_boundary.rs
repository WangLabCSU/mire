//! FASTQ reading and read transformations through the public API.
use std::io::Write;

use mire_sequence::read_fastq;
use mire_streaming::RecordSource;
use tempfile::NamedTempFile;

#[test]
fn paired_reader_preserves_end_order_and_fields() {
    let mut first = NamedTempFile::new().unwrap();
    let mut second = NamedTempFile::new().unwrap();
    first
        .write_all(b"@read MIRE{UMI:AC}\nACGT\n+\n1234\n")
        .unwrap();
    second.write_all(b"@read\nTGCA\n+\n5678\n").unwrap();
    let mut source = read_fastq(first.path(), Some(second.path())).unwrap();
    let pair = source.next_record().unwrap().unwrap();
    assert_eq!(pair.id(), b"read");
    assert_eq!(
        pair.first().description().unwrap().as_ref(),
        b"MIRE{UMI:AC}"
    );
    assert_eq!(pair.first().sequence().as_ref(), b"ACGT");
    assert_eq!(pair.second().unwrap().quality().as_ref(), b"5678");
    let (first, second) = pair.into_reads();
    assert_eq!(first.into_sequence_and_quality().1.as_ref(), b"1234");
    assert_eq!(
        second.unwrap().into_sequence_and_quality().0.as_ref(),
        b"TGCA"
    );
    assert!(source.next_record().unwrap().is_none());
}

#[test]
fn pairing_errors_remain_owned_by_sequence_reader() {
    let mut first = NamedTempFile::new().unwrap();
    first.write_all(b"@a\nAC\n+\nII\n").unwrap();
    for contents in [b"@b\nGT\n+\nII\n".as_slice(), b""] {
        let mut second = NamedTempFile::new().unwrap();
        second.write_all(contents).unwrap();
        let mut source = read_fastq(first.path(), Some(second.path())).unwrap();
        let error = source.next_record().unwrap_err().to_string();
        assert!(error.contains("FASTQ pairing error"));
        assert!(error.contains(first.path().to_str().unwrap()));
        assert!(error.contains(second.path().to_str().unwrap()));
    }
}
