use super::values::{strings_arg, u8_to_list_rstr, u8_to_rstr};
use extendr_api::prelude::*;
use mire_kreport::{Report, ReportRequest};

/// R-facing table representation of a parsed Kraken2 report.
struct RKrakenReportTable {
    percentages: Vec<f64>,
    clade_reads: Vec<f64>,
    direct_reads: Vec<f64>,
    minimizer_counts: Vec<Option<f64>>,
    distinct_minimizer_counts: Vec<Option<f64>>,
    ranks: Vec<Rstr>,
    taxids: Vec<Rstr>,
    taxa: Vec<Rstr>,
    lineage_ranks: Vec<Robj>,
    lineage_taxids: Vec<Robj>,
    lineage_taxa: Vec<Robj>,
    has_minimizer_data: bool,
}

impl From<Report> for RKrakenReportTable {
    fn from(report: Report) -> Self {
        let mut table = Self::with_capacity(report.len());

        for entry in report.into_rows() {
            let taxon = entry.taxon;
            let lineage = entry.lineage;
            let mut lineage_ranks = Vec::with_capacity(lineage.len());
            let mut lineage_taxids = Vec::with_capacity(lineage.len());
            let mut lineage_names = Vec::with_capacity(lineage.len());

            for taxon in lineage {
                lineage_ranks.push(taxon.rank.as_bytes().to_vec());
                lineage_taxids.push(taxon.taxid.as_bytes().to_vec());
                lineage_names.push(taxon.name.as_bytes().to_vec());
            }

            table.percentages.push(entry.percentage);
            table.clade_reads.push(entry.clade_reads as f64);
            table.direct_reads.push(entry.direct_reads as f64);
            table.has_minimizer_data |= entry.minimizer_count.is_some();
            table
                .minimizer_counts
                .push(entry.minimizer_count.map(|value| value as f64));
            table
                .distinct_minimizer_counts
                .push(entry.distinct_minimizer_count.map(|value| value as f64));
            table.ranks.push(u8_to_rstr(taxon.rank.as_bytes().to_vec()));
            table
                .taxids
                .push(u8_to_rstr(taxon.taxid.as_bytes().to_vec()));
            table.taxa.push(u8_to_rstr(taxon.name.as_bytes().to_vec()));
            table
                .lineage_ranks
                .push(Robj::from(u8_to_list_rstr(lineage_ranks)));
            table
                .lineage_taxids
                .push(Robj::from(u8_to_list_rstr(lineage_taxids)));
            table
                .lineage_taxa
                .push(Robj::from(u8_to_list_rstr(lineage_names)));
        }

        table
    }
}

impl RKrakenReportTable {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            percentages: Vec::with_capacity(capacity),
            clade_reads: Vec::with_capacity(capacity),
            direct_reads: Vec::with_capacity(capacity),
            minimizer_counts: Vec::with_capacity(capacity),
            distinct_minimizer_counts: Vec::with_capacity(capacity),
            ranks: Vec::with_capacity(capacity),
            taxids: Vec::with_capacity(capacity),
            taxa: Vec::with_capacity(capacity),
            lineage_ranks: Vec::with_capacity(capacity),
            lineage_taxids: Vec::with_capacity(capacity),
            lineage_taxa: Vec::with_capacity(capacity),
            has_minimizer_data: false,
        }
    }

    fn into_r_list(self) -> List {
        let ranks = List::from_values(self.lineage_ranks);
        let taxids = List::from_values(self.lineage_taxids);
        let taxa = List::from_values(self.lineage_taxa);

        if self.has_minimizer_data {
            list![
                percents = self.percentages,
                total_reads = self.clade_reads,
                reads = self.direct_reads,
                minimizer_len = self.minimizer_counts,
                minimizer_n_unique = self.distinct_minimizer_counts,
                rank = self.ranks,
                taxid = self.taxids,
                taxon = self.taxa,
                ranks = ranks,
                taxids = taxids,
                taxa = taxa
            ]
        } else {
            list![
                percents = self.percentages,
                total_reads = self.clade_reads,
                reads = self.direct_reads,
                rank = self.ranks,
                taxid = self.taxids,
                taxon = self.taxa,
                ranks = ranks,
                taxids = taxids,
                taxa = taxa
            ]
        }
    }
}

#[extendr]
fn read_kreport(kreport: &str, taxonomy: Robj) -> std::result::Result<List, String> {
    let report = ReportRequest {
        path: kreport,
        taxonomy: strings_arg(&taxonomy, "taxonomy")?,
    }
    .load()
    .map_err(|error| error.to_string())?;
    Ok(RKrakenReportTable::from(report).into_r_list())
}

extendr_module! {
    mod kreport;
    fn read_kreport;
}
