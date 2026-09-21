use super::{Taxid, Taxon, TaxonLevel};

/// Classified Kraken report entries in report order.
#[derive(Debug, PartialEq)]
pub struct KrakenReport {
    entries: Vec<KrakenReportEntry>,
}

impl KrakenReport {
    // Collect entries in their existing report order.
    pub(crate) fn new(entries: Vec<KrakenReportEntry>) -> Self {
        Self { entries }
    }

    /// Number of report entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the report has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Consume the report, preserving entry order.
    pub fn into_entries(self) -> Vec<KrakenReportEntry> {
        self.entries
    }

    /// Borrow report entries in input order.
    pub fn entries(&self) -> &[KrakenReportEntry] {
        &self.entries
    }

    /// Taxonomic IDs in report order.
    pub fn taxids(&self) -> impl Iterator<Item = &Taxid> {
        self.entries.iter().map(|entry| entry.taxon().taxid())
    }

    /// Taxonomic levels in report order, including repeated levels.
    pub fn ranks(&self) -> impl Iterator<Item = &TaxonLevel> {
        self.entries.iter().map(|entry| entry.taxon().level())
    }

    /// Scientific names in report order.
    pub fn taxa(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.taxon().term())
    }
}

/// A classified taxon with its reported abundance and ancestor lineage.
#[derive(Clone, Debug, PartialEq)]
pub struct KrakenReportEntry {
    percentage: f64,
    clade_reads: usize,
    direct_reads: usize,
    minimizer_count: Option<usize>,
    distinct_minimizer_count: Option<usize>,
    taxon: Taxon,
    lineage: Vec<Taxon>,
    hierarchy_depth: usize,
}

/// The owned abundance, taxon and lineage values of a report entry.
pub struct KrakenReportEntryParts {
    /// Percentage of fragments covered by this taxon's clade.
    pub percentage: f64,
    /// Fragments assigned to this taxon or its descendants.
    pub clade_reads: usize,
    /// Fragments assigned directly to this taxon.
    pub direct_reads: usize,
    /// Minimizers reported for this clade, when present.
    pub minimizer_count: Option<usize>,
    /// Estimated distinct minimizers, when present.
    pub distinct_minimizer_count: Option<usize>,
    /// The classified taxon.
    pub taxon: Taxon,
    /// Ancestors in report order, excluding this taxon and root taxa.
    pub lineage: Vec<Taxon>,
    /// Depth in the reported taxonomic hierarchy.
    pub hierarchy_depth: usize,
}

impl KrakenReportEntry {
    // Construct an entry with an ancestor-only lineage and its hierarchy depth.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        percentage: f64,
        clade_reads: usize,
        direct_reads: usize,
        minimizer_count: Option<usize>,
        distinct_minimizer_count: Option<usize>,
        taxon: Taxon,
        lineage: Vec<Taxon>,
        hierarchy_depth: usize,
    ) -> Self {
        Self {
            percentage,
            clade_reads,
            direct_reads,
            minimizer_count,
            distinct_minimizer_count,
            taxon,
            lineage,
            hierarchy_depth,
        }
    }

    /// Number of minimizers when provided by an eight-column report.
    pub fn minimizer_count(&self) -> Option<usize> {
        self.minimizer_count
    }

    /// Estimated number of distinct minimizers, when provided.
    pub fn distinct_minimizer_count(&self) -> Option<usize> {
        self.distinct_minimizer_count
    }

    /// The taxon classified by this entry.
    pub fn taxon(&self) -> &Taxon {
        &self.taxon
    }

    /// Ancestors only, excluding the current taxon and the root rank.
    pub fn lineage(&self) -> &[Taxon] {
        &self.lineage
    }

    /// Depth in the reported taxonomic hierarchy.
    pub fn hierarchy_depth(&self) -> usize {
        self.hierarchy_depth
    }

    /// Consume the entry into its owned report fields.
    pub fn into_parts(self) -> KrakenReportEntryParts {
        KrakenReportEntryParts {
            percentage: self.percentage,
            clade_reads: self.clade_reads,
            direct_reads: self.direct_reads,
            minimizer_count: self.minimizer_count,
            distinct_minimizer_count: self.distinct_minimizer_count,
            taxon: self.taxon,
            lineage: self.lineage,
            hierarchy_depth: self.hierarchy_depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{KrakenReport, KrakenReportEntry, Taxid, Taxon, TaxonLevel};

    fn taxon(level: &str, taxid: &str, term: &str) -> Taxon {
        Taxon::new(
            TaxonLevel::parse(level).unwrap(),
            Taxid::new(taxid.to_owned()).unwrap(),
            term.to_owned(),
        )
    }

    #[test]
    fn empty_report_accessors_are_empty() {
        let report = KrakenReport::new(Vec::new());
        assert!(report.is_empty());
        assert_eq!(report.len(), 0);
        assert!(report.entries().is_empty());
        assert_eq!(report.taxids().count(), 0);
        assert_eq!(report.ranks().count(), 0);
        assert_eq!(report.taxa().count(), 0);
        assert!(report.into_entries().is_empty());
    }

    #[test]
    fn report_accessors_preserve_entry_order_and_lineages() {
        let bacteria = taxon("D", "2", "Bacteria");
        let genus = taxon("G", "10", "Genus");
        let entries = vec![
            KrakenReportEntry::new(
                100.0,
                4,
                0,
                Some(30),
                Some(7),
                genus.clone(),
                vec![bacteria.clone()],
                2,
            ),
            KrakenReportEntry::new(
                50.0,
                2,
                2,
                None,
                None,
                taxon("S", "11", "Species A"),
                vec![bacteria.clone(), genus.clone()],
                3,
            ),
            KrakenReportEntry::new(
                50.0,
                2,
                2,
                None,
                None,
                taxon("S", "12", "Species B"),
                vec![bacteria, genus],
                3,
            ),
        ];
        let report = KrakenReport::new(entries.clone());
        assert_eq!(report.len(), 3);
        assert!(!report.is_empty());
        assert_eq!(report.entries(), entries);
        assert_eq!(
            report.taxids().map(Taxid::as_str).collect::<Vec<_>>(),
            ["10", "11", "12"]
        );
        assert_eq!(
            report.ranks().map(ToString::to_string).collect::<Vec<_>>(),
            ["G", "S", "S"]
        );
        assert_eq!(
            report.taxa().collect::<Vec<_>>(),
            ["Genus", "Species A", "Species B"]
        );
        assert_eq!(
            report.entries()[1]
                .lineage()
                .iter()
                .map(|taxon| taxon.taxid().as_str())
                .collect::<Vec<_>>(),
            ["2", "10"]
        );
        assert_eq!(report.into_entries(), entries);
    }

    #[test]
    fn entry_parts_preserve_report_fields() {
        let genus = taxon("G", "10", "Genus");
        let lineage = vec![taxon("D", "2", "Bacteria")];
        for (minimizers, distinct_minimizers) in [(None, None), (Some(30), Some(7))] {
            let entry = KrakenReportEntry::new(
                100.0,
                4,
                2,
                minimizers,
                distinct_minimizers,
                genus.clone(),
                lineage.clone(),
                1,
            );
            let parts = entry.into_parts();
            assert_eq!(parts.percentage, 100.0);
            assert_eq!(parts.clade_reads, 4);
            assert_eq!(parts.direct_reads, 2);
            assert_eq!(parts.minimizer_count, minimizers);
            assert_eq!(parts.distinct_minimizer_count, distinct_minimizers);
            assert_eq!(parts.taxon, genus);
            assert_eq!(parts.lineage, lineage);
            assert_eq!(parts.hierarchy_depth, 1);
        }
    }
}
