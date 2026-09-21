use bytes::Bytes;
use mire_streaming::{Result, WorkflowError};

use super::domain::statistics::BarcodeCounts;

pub type CountColumn = Vec<Option<usize>>;

/// Read and k-mer counts by taxon and barcode, with rows in Kraken report order.
pub struct CountTables {
    pub ranks: Vec<String>,
    pub taxa: Vec<Vec<Option<String>>>,
    pub barcodes: Vec<String>,
    pub counts: Vec<CountColumn>,
    pub kmer_total: Vec<CountColumn>,
    pub kmer_unique: Vec<CountColumn>,
}

impl CountTables {
    pub(crate) fn build(
        taxids: &[Bytes],
        ranks: Vec<String>,
        taxa: Vec<Vec<Option<String>>>,
        counts: &BarcodeCounts,
    ) -> Result<Self> {
        let mut table = Self {
            ranks,
            taxa,
            barcodes: Vec::new(),
            counts: Vec::new(),
            kmer_total: Vec::new(),
            kmer_unique: Vec::new(),
        };
        for (barcode, taxa) in counts {
            table.barcodes.push(
                String::from_utf8(barcode.to_vec()).map_err(|error| {
                    WorkflowError::operation("barcode must be valid UTF-8", error)
                })?,
            );
            let mut reads = Vec::with_capacity(taxids.len());
            let mut total = Vec::with_capacity(taxids.len());
            let mut unique = Vec::with_capacity(taxids.len());
            for taxid in taxids {
                let stats = taxa.get(taxid);
                reads.push(stats.map(|stats| stats.reads()));
                total.push(stats.map(|stats| stats.kmer_total()));
                unique.push(stats.map(|stats| stats.kmer_unique()));
            }
            table.counts.push(reads);
            table.kmer_total.push(total);
            table.kmer_unique.push(unique);
        }
        Ok(table)
    }
}
