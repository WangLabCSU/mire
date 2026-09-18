use super::domain::statistics::BarcodeCounts;
use mire_kreport::Report;
use mire_streaming::{Result, WorkflowError};

pub type CountColumn = Vec<Option<usize>>;

/// An R-independent projection; rows retain Kraken report order.
pub struct CountTables {
    pub ranks: Vec<String>,
    pub taxa: Vec<Vec<Option<String>>>,
    pub barcodes: Vec<String>,
    pub counts: Vec<CountColumn>,
    pub kmer_total: Vec<CountColumn>,
    pub kmer_unique: Vec<CountColumn>,
}

impl CountTables {
    pub(crate) fn build(reports: &Report, counts: &BarcodeCounts) -> Result<Self> {
        let (ranks, taxa) = reports.lineage_columns();
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
            let mut reads = Vec::with_capacity(reports.len());
            let mut total = Vec::with_capacity(reports.len());
            let mut unique = Vec::with_capacity(reports.len());
            for taxid in reports.taxids() {
                let stats = taxa.get(taxid.as_bytes());
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
