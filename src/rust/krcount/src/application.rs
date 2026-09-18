use bytes::Bytes;

use super::domain::{
    kmer::extract_kmers,
    record::{CountError, CountRecord},
    statistics::BarcodeCounts,
};
use mire_kreport::Report;
use mire_streaming::{RecordExecutor, RecordSink, RecordSource, Result};

pub(crate) struct CountReads<'a> {
    taxonomy: &'a Report,
    umi_tag: Option<String>,
    barcode_tag: Option<String>,
}

struct ReadContribution {
    barcode: Bytes,
    umi: Option<Bytes>,
    ancestors: Vec<Bytes>,
    kmers: Vec<Bytes>,
}

impl<'a> CountReads<'a> {
    pub(crate) fn new(
        taxonomy: &'a Report,
        umi_tag: Option<String>,
        barcode_tag: Option<String>,
    ) -> Self {
        Self {
            taxonomy,
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
        let Some(ancestors) = self.taxonomy.ancestors(record.taxid) else {
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
