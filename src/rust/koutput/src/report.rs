use std::io::Read;

use kreport::{Error, KrakenReportReader};

pub(crate) struct ReportTaxon {
    pub(crate) taxid: String,
    pub(crate) rank: String,
    pub(crate) term: String,
    pub(crate) lineage: Vec<String>,
}

// Translate report entries into the values needed for classification selection.
pub(crate) fn read_taxa<R: Read>(
    mut reader: KrakenReportReader<R>,
) -> Result<Vec<ReportTaxon>, Error> {
    reader
        .entries()
        .map(|entry| {
            entry.map(|entry| ReportTaxon {
                taxid: entry.taxon().taxid().as_str().to_owned(),
                rank: entry.taxon().level().to_string(),
                term: entry.taxon().term().to_owned(),
                lineage: entry
                    .lineage()
                    .iter()
                    .map(|taxon| taxon.taxid().as_str().to_owned())
                    .collect(),
            })
        })
        .collect()
}
