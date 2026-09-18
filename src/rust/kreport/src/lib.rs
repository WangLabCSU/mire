//! Kraken reports own parsing, lineage reconstruction and taxon selection.
//! Callers use this facade; file readers, builders and domain entities are private.
//! This workspace member has no dependency on the workflow core or language adapters.
//!
//! Internal parsing machinery is intentionally inaccessible to downstream crates:
//! ```compile_fail
//! use mire_kreport::domain::parser::KrakenReportParser;
//! ```
mod application;
mod domain;
mod error;
mod file;
mod reader;
mod selection;

use bytes::Bytes;
use rustc_hash::FxHashSet as HashSet;
use std::path::Path;

pub use domain::error::{KrakenReportError as ParseError, TaxonomyError};
use domain::model::{KrakenReportEntry, Taxon};
pub use error::{ReportError, Result};
pub use selection::TaxonSelection;
use selection::{rank_order_key, TaxonomyIndex};

/// A file report request, optionally restricted to a taxonomy lineage.
pub struct ReportRequest<'a> {
    pub path: &'a str,
    pub taxonomy: Option<Vec<String>>,
}

impl ReportRequest<'_> {
    /// Read and select the report. Returns contextual I/O or report validation errors.
    pub fn load(&self) -> Result<Report> {
        let taxonomy = self
            .taxonomy
            .as_ref()
            .map(|labels| labels.iter().map(String::as_str).collect::<Vec<_>>());
        read(self.path, taxonomy.as_deref())
    }
}

/// Read a six- or eight-column Kraken report, optionally selecting a taxonomy.
///
/// # Errors
/// Returns an error for unreadable, empty or malformed reports and invalid taxonomy selections.
///
/// ```no_run
/// let report = mire_kreport::read("sample.kreport", Some(&["D__Bacteria"]))?;
/// assert!(!report.is_empty());
/// # Ok::<(), mire_kreport::ReportError>(())
/// ```
pub fn read(path: impl AsRef<Path>, taxonomy: Option<&[&str]>) -> Result<Report> {
    let report = application::read_report(&file::KrakenReportFile::new(path), taxonomy)?;
    let entries = report.into_entries();
    let taxonomy = TaxonomyIndex::new(&entries);
    Ok(Report { entries, taxonomy })
}

/// A report with a private lineage index. Iteration preserves report row order.
pub struct Report {
    entries: Vec<KrakenReportEntry>,
    taxonomy: TaxonomyIndex,
}

impl Report {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Taxonomic IDs in report order.
    pub fn taxids(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.taxon().taxid())
    }
    /// Lineage membership, including the taxon itself where the report supplies it.
    pub fn ancestors(&self, taxid: &[u8]) -> Option<&HashSet<Bytes>> {
        self.taxonomy.ancestors(taxid)
    }
    /// Apply rank, name and taxid filters and optionally include descendants.
    pub fn select_taxids(&self, selection: &TaxonSelection) -> HashSet<Bytes> {
        selection.select(&self.entries)
    }
    /// Project lineage names into rank columns in biological rank order.
    pub fn lineage_columns(&self) -> (Vec<String>, Vec<Vec<Option<String>>>) {
        let mut ranks: Vec<_> = self
            .entries
            .iter()
            .flat_map(|entry| entry.lineage().iter().map(|taxon| taxon.rank()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        ranks.sort_by_key(|rank| (rank_order_key(rank), *rank));
        let columns = ranks
            .iter()
            .map(|rank| {
                self.entries
                    .iter()
                    .map(|entry| {
                        entry
                            .lineage()
                            .iter()
                            .find(|taxon| taxon.rank() == *rank)
                            .map(|taxon| taxon.name().to_owned())
                    })
                    .collect()
            })
            .collect();
        (ranks.into_iter().map(str::to_owned).collect(), columns)
    }
    /// Consume the report into transport rows without exposing internal entities.
    pub fn into_rows(self) -> Vec<ReportRow> {
        self.entries
            .into_iter()
            .map(|entry| {
                let entry = entry.into_parts();
                ReportRow {
                    percentage: entry.percentage,
                    clade_reads: entry.clade_reads,
                    direct_reads: entry.direct_reads,
                    minimizer_count: entry.minimizer_count,
                    distinct_minimizer_count: entry.distinct_minimizer_count,
                    taxon: TaxonLabel::from(entry.taxon),
                    lineage: entry.lineage.into_iter().map(TaxonLabel::from).collect(),
                }
            })
            .collect()
    }
}

/// An output projection, independent of any language binding or table library.
pub struct ReportRow {
    pub percentage: f64,
    pub clade_reads: usize,
    pub direct_reads: usize,
    pub minimizer_count: Option<usize>,
    pub distinct_minimizer_count: Option<usize>,
    pub taxon: TaxonLabel,
    pub lineage: Vec<TaxonLabel>,
}

/// Taxonomic labels for presentation or interchange.
pub struct TaxonLabel {
    pub rank: String,
    pub taxid: String,
    pub name: String,
}
impl From<Taxon> for TaxonLabel {
    fn from(taxon: Taxon) -> Self {
        Self {
            rank: taxon.rank().to_owned(),
            taxid: taxon.taxid().to_owned(),
            name: taxon.name().to_owned(),
        }
    }
}
