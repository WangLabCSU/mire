use bytes::Bytes;
use mire_streaming::{RecordExecutor, RecordSink, RecordSource, Result};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

use super::domain::{
    kmer::extract_kmers,
    record::{CountError, CountRecord},
    statistics::BarcodeCounts,
};

pub(crate) struct CountReads {
    ancestors: HashMap<Bytes, HashSet<Bytes>>,
    umi_tag: Option<String>,
    barcode_tag: Option<String>,
}

struct ReadContribution {
    barcode: Bytes,
    umi: Option<Bytes>,
    ancestors: Vec<Bytes>,
    kmers: Vec<Bytes>,
}

impl CountReads {
    pub(crate) fn new(
        ancestors: HashMap<Bytes, HashSet<Bytes>>,
        umi_tag: Option<String>,
        barcode_tag: Option<String>,
    ) -> Self {
        Self {
            ancestors,
            umi_tag,
            barcode_tag,
        }
    }

    fn contribution(
        &self,
        line: Bytes,
    ) -> std::result::Result<Option<ReadContribution>, CountError> {
        let Some(record) = CountRecord::parse(&line)? else {
            return Ok(None);
        };
        if !record.passes_filters() {
            return Ok(None);
        }
        let Some(ancestors) = self.ancestors.get(record.taxid) else {
            return Ok(None);
        };
        Ok(Some(ReadContribution {
            barcode: record
                .tag(self.barcode_tag.as_deref())?
                .map(Bytes::copy_from_slice)
                .unwrap_or_default(),
            umi: record
                .tag(self.umi_tag.as_deref())?
                .map(Bytes::copy_from_slice),
            ancestors: ancestors.iter().cloned().collect(),
            kmers: extract_kmers(record.lca, &record.sequences)?,
        }))
    }

    pub(crate) fn execute<S, X>(&self, source: S, executor: &X) -> Result<BarcodeCounts>
    where
        S: RecordSource<Record = Bytes> + Send,
        X: RecordExecutor,
    {
        let mut counts = CountAccumulator(BarcodeCounts::default());
        executor.execute(source, &mut counts, |line| self.contribution(line))?;
        Ok(counts.0)
    }
}

struct CountAccumulator(BarcodeCounts);

impl RecordSink<ReadContribution> for CountAccumulator {
    fn write_record(&mut self, contribution: ReadContribution) -> Result<()> {
        let taxa = self.0.entry(contribution.barcode).or_default();
        for ancestor in contribution.ancestors {
            taxa.entry(ancestor)
                .or_default()
                .add(&contribution.kmers, contribution.umi.as_ref());
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use kreport::KrakenReportReader;

    use super::CountReads;
    use crate::report::read_taxonomy;

    #[test]
    fn contributions_use_the_last_ancestry_for_duplicate_taxids() {
        let input = b"100\t4\t0\tR\t1\troot\n100\t4\t0\tD\t2\t  Bacteria\n100\t4\t0\tG\t10\t    First genus\n100\t4\t4\tS\t11\t      Species\n100\t4\t0\tG\t20\t    Last genus\n100\t4\t4\tS\t11\t      Species\n";
        let report = read_taxonomy(KrakenReportReader::new(input.as_slice())).unwrap();
        let count = CountReads::new(report.ancestors, None, None);
        let contribution = count
            .contribution(Bytes::from_static(b"11\t\t11:2\tACGT\tIIII"))
            .unwrap()
            .unwrap();
        assert_eq!(contribution.ancestors.len(), 3);
        for taxid in ["2", "20", "11"] {
            assert!(contribution
                .ancestors
                .iter()
                .any(|ancestor| ancestor.as_ref() == taxid.as_bytes()));
        }
    }
}
