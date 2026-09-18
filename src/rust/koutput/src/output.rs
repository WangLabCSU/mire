use crate::ClassifiedRead;
use mire_streaming::{ChunkWriter, RecordSink, Result};

pub(crate) struct ClassifiedReadSink {
    pub(crate) writer: ChunkWriter,
    pub(crate) buffer: Vec<u8>,
}

impl RecordSink<ClassifiedRead> for ClassifiedReadSink {
    fn write_record(&mut self, read: ClassifiedRead) -> Result<()> {
        let out = &mut self.buffer;
        out.clear();
        out.extend_from_slice(&read.taxid);
        out.push(b'\t');
        for (index, (tag, value)) in read.tags.iter().enumerate() {
            if index > 0 {
                out.push(b' ');
            }
            out.extend_from_slice(tag);
            out.push(b':');
            out.extend_from_slice(value);
        }
        out.push(b'\t');
        out.extend_from_slice(&read.lca);
        out.push(b'\t');
        if let Some(first) = &read.first {
            out.extend_from_slice(&first.sequence);
            out.push(b' ');
        }
        out.extend_from_slice(&read.last.sequence);
        out.push(b'\t');
        if let Some(first) = &read.first {
            out.extend_from_slice(&first.quality);
            out.push(b' ');
        }
        out.extend_from_slice(&read.last.quality);
        out.push(b'\n');
        self.writer.write_bytes(out)
    }

    fn finish(&mut self) -> Result<()> {
        self.writer.finish()
    }
}
