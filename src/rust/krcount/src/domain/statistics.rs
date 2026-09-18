use bytes::Bytes;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

/// Statistics of reads assigned to a taxon within one barcode group.
#[derive(Default, Debug)]
pub(crate) struct ReadCounts {
    reads: usize,
    kmer_total: usize,
    kmers: HashSet<Bytes>,
    umis: HashSet<Bytes>,
}

impl ReadCounts {
    pub(crate) fn add(&mut self, kmers: &[Bytes], umi: Option<&Bytes>) {
        self.reads += 1;
        self.kmer_total += kmers.len();
        self.kmers.extend(kmers.iter().cloned());
        if let Some(umi) = umi {
            self.umis.insert(umi.clone());
        }
    }

    pub(crate) fn reads(&self) -> usize {
        self.reads
    }
    pub(crate) fn kmer_total(&self) -> usize {
        self.kmer_total
    }
    pub(crate) fn kmer_unique(&self) -> usize {
        self.kmers.len()
    }
}

pub(crate) type BarcodeCounts = HashMap<Bytes, HashMap<Bytes, ReadCounts>>;
