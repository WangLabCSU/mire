use crate::FastqRecord;
#[cfg(test)]
use std::io::Write;

impl<T: AsRef<[u8]>> FastqRecord<T> {
    #[cfg(test)]
    pub(crate) fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&self.as_vec())
    }

    #[cfg(test)]
    pub(crate) fn bytes_size(&self) -> usize {
        self.id.as_ref().len()
            // extra one for space between id and description
            + self.desc.as_ref().map(|d| d.as_ref().len() + 1).unwrap_or(0) // ' '
            + self.seq.as_ref().len()
            + self.sep.as_ref().len()
            + self.qual.as_ref().len()
            + 5 // '@' and 4 * '\n'
    }

    /// Efficiently appends the FASTQ record to the provided Vec<u8>
    pub(crate) fn extend(&self, buf: &mut Vec<u8>) {
        buf.push(b'@');
        buf.extend_from_slice(self.id.as_ref());

        if let Some(desc) = &self.desc {
            buf.push(b' ');
            buf.extend_from_slice(desc.as_ref());
        }

        buf.push(b'\n');
        buf.extend_from_slice(self.seq.as_ref());
        buf.push(b'\n');
        buf.extend_from_slice(self.sep.as_ref());
        buf.push(b'\n');
        buf.extend_from_slice(self.qual.as_ref());
        buf.push(b'\n');
    }

    #[cfg(test)]
    pub(crate) fn as_vec(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(self.bytes_size());
        self.extend(&mut buffer);
        buffer
    }
}
#[cfg(test)]
mod test_record {
    use std::io::Cursor;

    use super::*; // Import FastqRecord

    #[test]
    fn test_fastq_record_write_with_description() {
        let record = FastqRecord::new(
            b"SEQ_ID".as_ref(),
            Some(b"desc".as_ref()),
            b"ACGTACGT".as_ref(),
            b"+".as_ref(),
            b"IIIIIIII".as_ref(),
        );

        let mut output = Cursor::new(Vec::new());
        record.write(&mut output).expect("Write failed");

        let expected = b"@SEQ_ID desc\nACGTACGT\n+\nIIIIIIII\n";
        assert_eq!(output.into_inner(), expected);
    }

    #[test]
    fn test_fastq_record_write_without_description() {
        let record = FastqRecord::new(
            b"SEQ_ID".as_ref(),
            None,
            b"ACGTACGT".as_ref(),
            b"+".as_ref(),
            b"IIIIIIII".as_ref(),
        );

        let mut output = Cursor::new(Vec::new());
        record.write(&mut output).expect("Write failed");

        let expected = b"@SEQ_ID\nACGTACGT\n+\nIIIIIIII\n";
        assert_eq!(output.into_inner(), expected);
    }
}
