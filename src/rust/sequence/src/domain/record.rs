/// A sequencing read's FASTQ identifier, description, bases and quality scores.
#[derive(Debug)]
pub struct FastqRecord<T> {
    pub(crate) id: T,
    pub(crate) desc: Option<T>,
    pub(crate) seq: T,
    pub(crate) sep: T,
    pub(crate) qual: T,
}

impl<T> FastqRecord<T> {
    /// Optional header description, without the ID or separating whitespace.
    pub fn description(&self) -> Option<&T> {
        self.desc.as_ref()
    }

    /// Bases in input order.
    pub fn sequence(&self) -> &T {
        &self.seq
    }

    /// Quality scores in the same order as the bases.
    pub fn quality(&self) -> &T {
        &self.qual
    }

    /// Consume the record and transfer its sequence and quality without copying.
    pub fn into_sequence_and_quality(self) -> (T, T) {
        (self.seq, self.qual)
    }

    #[allow(dead_code)]
    pub(crate) fn new(id: T, desc: Option<T>, seq: T, sep: T, qual: T) -> Self {
        Self {
            id,
            desc,
            seq,
            sep,
            qual,
        }
    }
}

impl<T: AsRef<[u8]>> FastqRecord<T> {
    #[allow(dead_code)]
    pub(crate) fn as_ref(&self) -> FastqRecord<&[u8]> {
        FastqRecord {
            id: self.id.as_ref(),
            desc: self.desc.as_ref().map(|d| d.as_ref()),
            seq: self.seq.as_ref(),
            sep: self.sep.as_ref(),
            qual: self.qual.as_ref(),
        }
    }
}
