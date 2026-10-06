use std::{
    slice::{Iter, IterMut},
    vec::IntoIter,
};

use super::Taxon;

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

    /// Iterate over borrowed entries in report order.
    ///
    /// ```
    /// use kreport::load_kreport;
    ///
    /// let input = b"100\t2\t0\tD\t2\tBacteria\n100\t2\t2\tG\t10\t  Genus\n";
    /// let report = load_kreport(input.as_slice(), Default::default())?;
    /// let names: Vec<_> = report.iter().map(|entry| entry.taxon().term()).collect();
    /// assert_eq!(names, ["Bacteria", "Genus"]);
    /// # Ok::<(), kreport::Error>(())
    /// ```
    pub fn iter(&self) -> Iter<'_, KrakenReportEntry> {
        self.entries.iter()
    }

    /// Iterate over mutably borrowed entries in report order.
    ///
    /// Entries can be replaced with updated report entries.
    ///
    /// ```
    /// use kreport::load_kreport;
    ///
    /// let input = b"100\t2\t2\tD\t2\tBacteria\n";
    /// let updated_input = b"100\t3\t3\tD\t2\tBacteria\n";
    /// let mut report = load_kreport(input.as_slice(), Default::default())?;
    /// let updated = load_kreport(updated_input.as_slice(), Default::default())?;
    /// for (entry, replacement) in report.iter_mut().zip(updated) {
    ///     *entry = replacement;
    /// }
    /// let entry = report.into_iter().next().unwrap().into_parts();
    /// assert_eq!(entry.clade_reads, 3);
    /// # Ok::<(), kreport::Error>(())
    /// ```
    pub fn iter_mut(&mut self) -> IterMut<'_, KrakenReportEntry> {
        self.entries.iter_mut()
    }
}

/// Iterate over borrowed entries in report order with `for entry in &report`.
impl<'a> IntoIterator for &'a KrakenReport {
    type Item = &'a KrakenReportEntry;
    type IntoIter = Iter<'a, KrakenReportEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterate over mutably borrowed entries in report order with `for entry in &mut report`.
impl<'a> IntoIterator for &'a mut KrakenReport {
    type Item = &'a mut KrakenReportEntry;
    type IntoIter = IterMut<'a, KrakenReportEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Consume the report and yield its entries in report order.
///
/// ```
/// use kreport::load_kreport;
///
/// let input = b"100\t2\t2\tD\t2\tBacteria\n";
/// let report = load_kreport(input.as_slice(), Default::default())?;
/// let parts: Vec<_> = report.into_iter().map(|entry| entry.into_parts()).collect();
/// assert_eq!(parts[0].taxon.term(), "Bacteria");
/// # Ok::<(), kreport::Error>(())
/// ```
impl IntoIterator for KrakenReport {
    type Item = KrakenReportEntry;
    type IntoIter = IntoIter<KrakenReportEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
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
    /// Ancestors in report order, excluding this taxon and ancestors whose major
    /// rank is root, such as `R`, `R1` and `R2`.
    pub lineage: Vec<Taxon>,
}

impl KrakenReportEntry {
    // Construct an entry with an ancestor-only lineage.
    pub(crate) fn new(
        percentage: f64,
        clade_reads: usize,
        direct_reads: usize,
        minimizer_count: Option<usize>,
        distinct_minimizer_count: Option<usize>,
        taxon: Taxon,
        lineage: Vec<Taxon>,
    ) -> Self {
        Self {
            percentage,
            clade_reads,
            direct_reads,
            minimizer_count,
            distinct_minimizer_count,
            taxon,
            lineage,
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

    /// Ancestors in report order, excluding the current taxon and ancestors whose
    /// major rank is root, such as `R`, `R1` and `R2`.
    pub fn lineage(&self) -> &[Taxon] {
        &self.lineage
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
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::{KrakenReport, KrakenReportEntry};
    use crate::domain::{Taxid, Taxon, TaxonLevel};

    fn taxon(level: &str, taxid: &str, term: &str) -> Taxon {
        Taxon::new(
            TaxonLevel::parse(level).unwrap(),
            Taxid::new(taxid.to_owned()).unwrap(),
            term.to_owned(),
        )
    }

    #[test]
    fn empty_report_has_zero_length() {
        let report = KrakenReport::new(Vec::new());
        assert!(report.is_empty());
        assert_eq!(report.len(), 0);
    }

    #[test]
    fn report_iteration_borrows_entries_in_report_order() {
        let report = KrakenReport::new(vec![
            KrakenReportEntry::new(100.0, 2, 0, None, None, taxon("G", "10", "Genus"), vec![]),
            KrakenReportEntry::new(
                100.0,
                2,
                2,
                None,
                None,
                taxon("S", "11", "Species"),
                vec![taxon("G", "10", "Genus")],
            ),
        ]);

        for mut entries in [report.iter(), (&report).into_iter()] {
            assert!(ptr::eq(entries.next().unwrap(), &report.entries[0]));
            assert!(ptr::eq(entries.next().unwrap(), &report.entries[1]));
            assert!(entries.next().is_none());
        }
    }

    #[test]
    fn mutable_iteration_replaces_entries_in_report_order() {
        let genus = taxon("G", "10", "Genus");
        let species = taxon("S", "11", "Species");
        let original = vec![
            KrakenReportEntry::new(100.0, 2, 0, None, None, genus.clone(), vec![]),
            KrakenReportEntry::new(
                100.0,
                2,
                2,
                None,
                None,
                species.clone(),
                vec![genus.clone()],
            ),
        ];
        let replacements = vec![
            KrakenReportEntry::new(100.0, 4, 0, None, None, genus.clone(), vec![]),
            KrakenReportEntry::new(100.0, 4, 4, None, None, species, vec![genus]),
        ];

        for use_into_iter in [false, true] {
            let mut report = KrakenReport::new(original.clone());
            let entries = if use_into_iter {
                (&mut report).into_iter()
            } else {
                report.iter_mut()
            };
            for (entry, replacement) in entries.zip(&replacements) {
                *entry = replacement.clone();
            }

            assert_eq!(report.into_iter().collect::<Vec<_>>(), replacements);
        }
    }

    #[test]
    fn report_iteration_is_empty_for_an_empty_report() {
        let mut report = KrakenReport::new(Vec::new());

        assert!(report.iter().next().is_none());
        assert!((&report).into_iter().next().is_none());
        assert!(report.iter_mut().next().is_none());
        assert!((&mut report).into_iter().next().is_none());
        assert!(report.into_iter().next().is_none());
    }

    #[test]
    fn report_preserves_entry_count_order_and_lineages() {
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
            ),
            KrakenReportEntry::new(
                50.0,
                2,
                2,
                None,
                None,
                taxon("S", "11", "Species A"),
                vec![bacteria.clone(), genus.clone()],
            ),
            KrakenReportEntry::new(
                50.0,
                2,
                2,
                None,
                None,
                taxon("S", "12", "Species B"),
                vec![bacteria, genus],
            ),
        ];
        let report = KrakenReport::new(entries.clone());
        assert_eq!(report.len(), 3);
        assert!(!report.is_empty());
        assert!(report.iter().eq(entries.iter()));
        assert_eq!(report.into_iter().collect::<Vec<_>>(), entries);
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
            );
            let parts = entry.into_parts();
            assert_eq!(parts.percentage, 100.0);
            assert_eq!(parts.clade_reads, 4);
            assert_eq!(parts.direct_reads, 2);
            assert_eq!(parts.minimizer_count, minimizers);
            assert_eq!(parts.distinct_minimizer_count, distinct_minimizers);
            assert_eq!(parts.taxon, genus);
            assert_eq!(parts.lineage, lineage);
        }
    }
}
