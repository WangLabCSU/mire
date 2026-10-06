// Coordinate report input, entry parsing and selection in report order.
use std::io::{self, Read};

use bytes::{Bytes, BytesMut};
use memchr::memchr;
use rustc_hash::FxHashSet as HashSet;

use crate::domain::{
    EntrySpec, EntrySpecScope, KrakenReport, KrakenReportEntry, KrakenReportParser, LineagePath,
    TaxonSpec,
};
use crate::error::Error;

/// Read a six- or eight-column report from an input source.
///
/// An empty set of conditions includes all classified taxa. An entry is selected
/// when its taxon or an ancestor in its lineage matches a condition.
/// Supported forms include taxids (`562`),
/// full rank names (`Genus`), taxonomic levels (`G2`), scientific names (`Bacteria`)
/// and level-and-name conditions (`G__Genus group`). Rank names are case-sensitive.
/// `G__Genus group` includes that name at genus or an intermediate level below
/// genus; `G2__Genus group` requires exactly two levels below genus.
///
/// Create conditions with [`TaxonSpec::parse`] or `try_into()` before reading.
/// Empty strings and numeric taxids with leading zeros, such as `02`, are rejected. Unrecognized
/// levels or incomplete conditions, such as `G256`, `X__name` and `G__`, are treated
/// as scientific names.
///
/// Blank and unclassified rows are skipped. Root entries are included when
/// selected; ancestor lineages omit the current taxon and ancestors whose major
/// rank is root, such as `R`, `R1` and `R2`.
/// For example, taxid `1` selects the root entry alone. Use no conditions to read
/// every classified entry. No matching entries, including an empty input, produce
/// an empty report; use `report.is_empty()` to check.
///
/// # Errors
/// Returns an [`Error`] with the report line if reading or parsing an entry fails,
/// including malformed unclassified rows.
/// A missing parent or a second top-level entry is a parsing error.
///
/// # Examples
/// ```
/// use kreport::load_kreport;
/// let input = b"100\t2\t2\tD\t2\tBacteria\n";
/// let report = load_kreport(input.as_slice(), Default::default())?;
/// assert_eq!(report.taxa().collect::<Vec<_>>(), ["Bacteria"]);
/// # Ok::<(), kreport::Error>(())
/// ```
pub fn load_kreport<R: Read>(
    reader: R,
    filters: HashSet<TaxonSpec>,
) -> Result<KrakenReport, Error> {
    let mut reader = KrakenReportReader::with_filters(filters, reader);
    let entries = reader.entries().collect::<Result<Vec<_>, Error>>()?;
    Ok(KrakenReport::new(entries))
}

/// Read Kraken report entries with their ancestor lineages.
///
/// Conditions match an entry's taxon or an ancestor in its lineage.
/// Ancestor lineages omit the current taxon and ancestors whose major rank is
/// root, such as `R`, `R1` and `R2`.
/// With no conditions, all classified entries are returned, including root entries.
///
/// ```
/// use kreport::KrakenReportReader;
/// let mut reader = KrakenReportReader::new(&b"100\t2\t2\tD\t2\tBacteria\n"[..]);
/// let entry = reader.read_entry()?.unwrap();
/// assert_eq!(entry.taxon().term(), "Bacteria");
/// assert!(reader.read_entry()?.is_none());
/// # Ok::<(), kreport::Error>(())
/// ```
pub struct KrakenReportReader<R> {
    reader: LineReader<R>,
    parser: KrakenReportParser,
    // Preserve the full taxonomic path between report lines.
    path: LineagePath,
    entry_spec: EntrySpec,
}

impl<R: Read> KrakenReportReader<R> {
    const BUFFER_SIZE: usize = 4 * 1024 * 1024;

    /// Create a reader for all classified entries, including root entries.
    pub fn new(reader: R) -> Self {
        Self::with_capacity_and_filters(Self::BUFFER_SIZE, HashSet::default(), reader)
    }

    /// Create an unfiltered reader with the requested input capacity in bytes.
    pub fn with_capacity(capacity: usize, reader: R) -> Self {
        Self::with_capacity_and_filters(capacity, HashSet::default(), reader)
    }

    /// Create a reader selecting entries by their taxon or ancestor lineage.
    /// An empty set of conditions leaves the report unrestricted.
    ///
    /// ```
    /// use kreport::KrakenReportReader;
    /// let filters = ["G__Genus A"].into_iter().map(TryInto::try_into)
    ///     .collect::<Result<_, _>>()?;
    /// let input = b"100\t4\t0\tD\t2\tBacteria\n100\t4\t0\tG\t10\t  Genus A\n100\t4\t4\tS\t11\t    Species A\n";
    /// let mut reader = KrakenReportReader::with_filters(filters, input.as_slice());
    /// let genus = reader.read_entry()?.unwrap();
    /// assert_eq!(genus.taxon().term(), "Genus A");
    /// assert_eq!(genus.lineage()[0].term(), "Bacteria");
    /// assert_eq!(reader.read_entry()?.unwrap().taxon().term(), "Species A");
    /// assert!(reader.read_entry()?.is_none());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_filters(filters: HashSet<TaxonSpec>, reader: R) -> Self {
        Self::with_capacity_and_filters(Self::BUFFER_SIZE, filters, reader)
    }

    /// Select entries by their taxon or ancestor lineage with the requested input capacity in bytes.
    /// An empty set of conditions leaves the report unrestricted.
    pub fn with_capacity_and_filters(
        capacity: usize,
        taxon_specs: HashSet<TaxonSpec>,
        reader: R,
    ) -> Self {
        Self {
            reader: LineReader::with_capacity(capacity, reader),
            parser: KrakenReportParser::new(),
            path: LineagePath::with_capacity(10),
            entry_spec: EntrySpec::new(taxon_specs, EntrySpecScope::Lineage),
        }
    }

    /// Number of physical lines read, including blank, unclassified and unselected rows.
    pub fn offset(&self) -> usize {
        self.reader.offset()
    }

    /// Read the next selected entry with its ancestor lineage.
    /// Blank and unclassified rows are skipped. Ancestor lineages omit the current
    /// taxon and ancestors whose major rank is root, such as `R`, `R1` and `R2`.
    /// `Ok(None)` means no selected entries remain.
    ///
    /// # Errors
    ///
    /// Returns an [`Error`] if reading or parsing an entry fails. Taxonomy
    /// conditions do not suppress errors in unselected rows. Malformed
    /// unclassified rows also produce errors.
    /// A missing parent or a second top-level entry is a parsing error.
    pub fn read_entry(&mut self) -> Result<Option<KrakenReportEntry>, Error> {
        while let Some(line) = self.reader.read_line() {
            // offset counts delivered lines; a read failure belongs to the next
            // physical line, including when part of that line is already buffered.
            let line = line.map_err(|source| Error::Read {
                line: self.offset() + 1,
                source,
            })?;
            // Parse every row before selection so unselected ancestors still
            // advance the path and malformed rows still report their errors.
            let Some(entry) = self
                .parser
                .parse_entry(&line, &mut self.path)
                .map_err(|source| Error::Parse {
                    // The line has been delivered, so offset now identifies it.
                    line: self.offset(),
                    source,
                })?
            else {
                continue;
            };
            if !self.entry_spec.is_satisfied_by(&entry) {
                continue;
            }
            return Ok(Some(entry));
        }
        Ok(None)
    }

    /// Iterate over the remaining selected entries in report order.
    /// Ancestor lineages omit the current taxon and ancestors whose major rank is
    /// root, such as `R`, `R1` and `R2`.
    ///
    /// # Errors
    ///
    /// Yields `Err(error)` if reading or parsing an entry fails.
    /// After a parsing error, iteration can continue with the following report line.
    ///
    /// ```
    /// use kreport::{Entries, KrakenReportReader};
    /// let input = b"100\t2\t2\tD\t2\tBacteria\n";
    /// let mut reader = KrakenReportReader::new(input.as_slice());
    /// let entries: Entries<'_, _> = reader.entries();
    /// for entry in entries {
    ///     println!("{}", entry?.taxon().term());
    /// }
    /// # Ok::<(), kreport::Error>(())
    /// ```
    pub fn entries(&mut self) -> Entries<'_, R> {
        Entries { reader: self }
    }
}

/// An iterator over selected report entries with their ancestor lineages, in report order.
pub struct Entries<'a, R> {
    reader: &'a mut KrakenReportReader<R>,
}

impl<R: Read> Iterator for Entries<'_, R> {
    type Item = Result<KrakenReportEntry, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // Only Ok(None) ends iteration; an error remains an item, allowing the
        // caller to request the following entry after a parsing failure.
        self.reader.read_entry().transpose()
    }
}

// Split buffered input into LF/CRLF lines. Keep unfinished line bytes across
// reads; remove terminators only once the complete line has been assembled.
struct LineReader<R> {
    reader: R,
    // Number of delivered physical lines, including empty lines.
    offset: usize,
    buffer_size: usize,
    // Unconsumed bytes from the current read; None triggers the next read.
    buffer: Option<BytesMut>,
    // Prefix of a line whose terminating LF has not yet been found.
    leftover: Option<BytesMut>,
}

impl<R: Read> LineReader<R> {
    fn with_capacity(capacity: usize, reader: R) -> Self {
        Self {
            reader,
            offset: 0,
            buffer: None,
            // A zero-length read buffer could make nonempty input look exhausted.
            buffer_size: capacity.max(1),
            leftover: None,
        }
    }

    fn offset(&self) -> usize {
        self.offset
    }

    #[inline]
    fn read_line(&mut self) -> Option<io::Result<Bytes>> {
        loop {
            if let Err(err) = self.fill_buf() {
                return Some(Err(err));
            };
            if let Some(buffer) = self.buffer.as_mut() {
                if let Some(pos) = memchr(b'\n', buffer) {
                    // Consume LF with the line so it cannot produce an extra
                    // empty line on the next call; leave later bytes buffered.
                    let mut buf = buffer.split_to(pos + 1);
                    let mut line = if let Some(mut leftover) = self.leftover.take() {
                        leftover.extend_from_slice(&buf[..pos]);
                        leftover
                    } else {
                        // Keep a slice of this buffer rather than copying the
                        // line bytes into a separate accumulation buffer.
                        buf.split_to(pos)
                    };
                    // CR and LF may arrive in different reads; strip CR after joining.
                    if line.last() == Some(&b'\r') {
                        line.truncate(line.len() - 1);
                    }
                    self.offset += 1;
                    return Some(Ok(line.freeze()));
                }

                // Without LF, retain this chunk as an unfinished line. Reuse the
                // first chunk directly and append subsequent chunks to it.
                if let Some(left) = self.leftover.as_mut() {
                    left.extend_from_slice(buffer);
                    self.buffer = None
                } else {
                    std::mem::swap(&mut self.buffer, &mut self.leftover);
                }
            } else {
                // At EOF, return a final unterminated line once. Its last byte
                // is content: a trailing CR is removed only as part of CRLF above.
                let left = self
                    .leftover
                    .take()
                    .filter(|line| !line.is_empty())
                    .map(|line| line.freeze());
                if left.is_some() {
                    self.offset += 1;
                }
                return left.map(Ok);
            }
        }
    }

    #[inline]
    fn fill_buf(&mut self) -> io::Result<()> {
        if self.buffer.is_none() {
            // Retain leftover across read errors so retrying can complete the
            // same physical line. Only successfully read bytes enter buffer.
            let mut buffer = BytesMut::zeroed(self.buffer_size);
            let nbytes = self.reader.read(&mut buffer)?;
            if nbytes > 0 {
                buffer.truncate(nbytes);
                self.buffer = Some(buffer)
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, io::Cursor};

    use super::*;

    const FILTER_REPORT: &[u8] = b"0\t0\t0\tU\t0\tunclassified\n100\t4\t0\tR\t1\troot\n100\t4\t0\tD\t2\t  Bacteria\n100\t4\t0\tG2\t10\t    Genus A\n50\t2\t2\tS\t11\t      Species A\n50\t2\t2\tS\t12\t      Species B\n100\t4\t0\tG\t20\t    Genus B\n100\t4\t4\tS\t21\t      Species C\n100\t4\t0\tD\t3\t  Archaea\n100\t4\t4\tS\t31\t    Species D\n";

    fn filters(labels: &[&str]) -> HashSet<TaxonSpec> {
        labels
            .iter()
            .map(|label| TaxonSpec::parse((*label).into()).unwrap())
            .collect()
    }

    #[test]
    fn partial_entry_iteration_preserves_reader_position_and_lineages() {
        let input = b"100\t4\t0\tD\t2\tBacteria\n100\t4\t0\tG\t10\t  Genus\n100\t4\t4\tS\t11\t    Species\n";
        let mut reader = KrakenReportReader::new(input.as_slice());
        let domain = reader.read_entry().unwrap().unwrap();
        assert_eq!(domain.taxon().taxid().as_str(), "2");

        let genus = reader.entries().next().unwrap().unwrap();
        assert_eq!(genus.taxon().taxid().as_str(), "10");
        assert_eq!(genus.lineage(), std::slice::from_ref(domain.taxon()));
        assert_eq!(reader.offset(), 2);

        let species = reader.read_entry().unwrap().unwrap();
        assert_eq!(species.taxon().taxid().as_str(), "11");
        assert_eq!(
            species.lineage(),
            [domain.taxon().clone(), genus.taxon().clone()]
        );
        assert_eq!(reader.offset(), 3);
        assert!(matches!(reader.read_entry(), Ok(None)));
    }

    #[test]
    fn entry_iteration_returns_parse_errors_and_preserves_ancestry_when_continued() {
        let input = b"100\t4\t0\tD\t2\tBacteria\nbroken\n100\t4\t4\tS\t11\t  Species\n";
        let mut reader = KrakenReportReader::new(input.as_slice());
        let mut entries = reader.entries();
        let domain = entries.next().unwrap().unwrap();
        assert!(matches!(
            entries.next(),
            Some(Err(Error::Parse { line: 2, .. }))
        ));
        let species = entries.next().unwrap().unwrap();
        assert_eq!(species.taxon().taxid().as_str(), "11");
        assert_eq!(species.lineage(), std::slice::from_ref(domain.taxon()));
        assert!(entries.next().is_none());
    }

    #[test]
    fn entry_selection_uses_the_new_parent_after_statistics_errors() {
        for (minimizers, invalid_statistics) in [
            ("", "invalid\t2\t0\t"),
            ("20\t5\t", "invalid\t2\t0\t20\t5\t"),
            ("20\t5\t", "50\t2\t0\tinvalid\t5\t"),
            ("20\t5\t", "50\t2\t0\t20\tinvalid\t"),
        ] {
            let input = format!(
                "100\t4\t0\t{minimizers}D\t2\tBacteria\n\
                 50\t2\t0\t{minimizers}G\t10\t  Genus A\n\
                 {invalid_statistics}G\t20\t  Genus B\n\
                 50\t2\t2\t{minimizers}S\t21\t    Species B\n"
            );
            let mut previous_branch =
                KrakenReportReader::with_filters(filters(&["10"]), input.as_bytes());
            assert_eq!(
                previous_branch
                    .read_entry()
                    .unwrap()
                    .unwrap()
                    .taxon()
                    .taxid()
                    .as_str(),
                "10"
            );
            assert!(matches!(
                previous_branch.read_entry(),
                Err(Error::Parse { line: 3, .. })
            ));
            assert!(previous_branch.read_entry().unwrap().is_none());

            let mut current_branch =
                KrakenReportReader::with_filters(filters(&["20"]), input.as_bytes());
            assert!(matches!(
                current_branch.read_entry(),
                Err(Error::Parse { line: 3, .. })
            ));
            let species = current_branch.read_entry().unwrap().unwrap();
            assert_eq!(species.taxon().taxid().as_str(), "21");
            assert_eq!(
                species
                    .lineage()
                    .iter()
                    .map(|taxon| taxon.taxid().as_str())
                    .collect::<Vec<_>>(),
                ["2", "20"]
            );
            assert!(current_branch.read_entry().unwrap().is_none());
        }
    }

    #[test]
    fn readers_keep_report_lineages_independent() {
        let mut bacteria = KrakenReportReader::new(
            b"100\t4\t0\tD\t2\tBacteria\n100\t4\t4\tS\t11\t  Species\n".as_slice(),
        );
        let mut archaea = KrakenReportReader::new(
            b"100\t4\t0\tD\t3\tArchaea\n100\t4\t4\tS\t31\t  Species\n".as_slice(),
        );
        for (reader, parent) in [(&mut bacteria, "2"), (&mut archaea, "3")] {
            let entry = reader.read_entry().unwrap().unwrap();
            assert_eq!(entry.taxon().taxid().as_str(), parent);
            assert!(entry.lineage().is_empty());
        }
        for (reader, parent) in [(&mut bacteria, "2"), (&mut archaea, "3")] {
            let species = reader.read_entry().unwrap().unwrap();
            assert_eq!(
                species
                    .lineage()
                    .iter()
                    .map(|taxon| taxon.taxid().as_str())
                    .collect::<Vec<_>>(),
                [parent]
            );
            assert!(reader.read_entry().unwrap().is_none());
        }
    }

    #[test]
    fn empty_filters_preserve_all_classified_entries() {
        let mut reader = KrakenReportReader::with_filters(HashSet::default(), FILTER_REPORT);
        let entries = reader.entries().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.taxon().taxid().as_str())
                .collect::<Vec<_>>(),
            ["1", "2", "10", "11", "12", "20", "21", "3", "31"]
        );
    }

    #[test]
    fn filters_preserve_descendants_lineages_and_report_order() {
        let mut reader =
            KrakenReportReader::with_filters(filters(&["12", "10", "10"]), FILTER_REPORT);
        let entries = reader.entries().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.taxon().taxid().as_str())
                .collect::<Vec<_>>(),
            ["10", "11", "12"]
        );
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry
                    .lineage()
                    .iter()
                    .map(|taxon| taxon.taxid().as_str())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            [vec!["2"], vec!["2", "10"], vec!["2", "10"]]
        );
        assert_eq!(reader.offset(), 10);
    }

    #[test]
    fn selecting_a_leaf_preserves_unselected_ancestors() {
        let mut reader = KrakenReportReader::with_filters(filters(&["11"]), FILTER_REPORT);
        let entry = reader.read_entry().unwrap().unwrap();
        assert_eq!(entry.taxon().taxid().as_str(), "11");
        assert_eq!(
            entry
                .lineage()
                .iter()
                .map(|taxon| taxon.taxid().as_str())
                .collect::<Vec<_>>(),
            ["2", "10"]
        );
        assert!(reader.read_entry().unwrap().is_none());
    }

    #[test]
    fn root_selection_matches_only_root_rank_entries() {
        for label in ["Root", "R", "1"] {
            let mut reader = KrakenReportReader::with_filters(filters(&[label]), FILTER_REPORT);
            let entries = reader.entries().collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry.taxon().taxid().as_str())
                    .collect::<Vec<_>>(),
                ["1"]
            );
        }
    }

    #[test]
    fn services_select_expected_taxa_and_preserve_entry_details() {
        let selections: &[(&[&str], &[&str])] = &[
            (&[], &["1", "2", "10", "11", "12", "20", "21", "3", "31"]),
            (&["0"], &[]),
            (&["1"], &["1"]),
            (&["2"], &["2", "10", "11", "12", "20", "21"]),
            (&["10"], &["10", "11", "12"]),
            (&["11"], &["11"]),
            (&["Species A"], &["11"]),
            (&["S__Species A"], &["11"]),
            (&["999"], &[]),
            (&["Root"], &["1"]),
            (&["Genus"], &["10", "11", "12", "20", "21"]),
            (&["G"], &["10", "11", "12", "20", "21"]),
            (&["G0"], &["10", "11", "12", "20", "21"]),
            (&["G2"], &["10", "11", "12"]),
            (&["Genus A"], &["10", "11", "12"]),
            (&["G__Genus A"], &["10", "11", "12"]),
            (&["G0__Genus A"], &["10", "11", "12"]),
            (&["G2__Genus A"], &["10", "11", "12"]),
            (&["G1__Genus A"], &[]),
            (&["12", "10", "10"], &["10", "11", "12"]),
            (&["S__Species B", "G__Genus A"], &["10", "11", "12"]),
            (&["3"], &["3", "31"]),
        ];
        for minimizers in [false, true] {
            let input = String::from_utf8(FILTER_REPORT.to_vec()).unwrap();
            let input = if minimizers {
                input
                    .lines()
                    .map(|line| {
                        let mut fields: Vec<_> = line.split('\t').collect();
                        fields.splice(3..3, ["20", "5"]);
                        fields.join("\t") + "\n"
                    })
                    .collect::<String>()
            } else {
                input
            };
            let report = load_kreport(input.as_bytes(), HashSet::default()).unwrap();
            assert_eq!(report.len(), 9);
            for (labels, taxids) in selections {
                let specs = filters(labels);
                let expected: Vec<_> = report
                    .entries()
                    .iter()
                    .filter(|entry| taxids.contains(&entry.taxon().taxid().as_str()))
                    .cloned()
                    .collect();
                assert_eq!(
                    load_kreport(input.as_bytes(), specs.clone())
                        .unwrap()
                        .into_entries(),
                    expected,
                    "{labels:?}, minimizers {minimizers}"
                );
                for capacity in [0, 1, 17, input.len()] {
                    let mut reader = KrakenReportReader::with_capacity_and_filters(
                        capacity,
                        specs.clone(),
                        input.as_bytes(),
                    );
                    let actual = reader.entries().collect::<Result<Vec<_>, _>>().unwrap();
                    assert_eq!(
                        actual, expected,
                        "{labels:?}, capacity {capacity}, minimizers {minimizers}"
                    );
                    assert_eq!(reader.offset(), 10);
                }
            }
        }
    }

    #[test]
    fn loading_returns_empty_reports_without_selected_entries() {
        for input in [
            b"".as_slice(),
            b" \n",
            b"0\t0\t0\tU\t0\tunclassified\n",
            FILTER_REPORT,
            b"100\t4\t0\tR\t1\troot\n",
        ] {
            assert!(load_kreport(input, filters(&["999"])).unwrap().is_empty());
        }
    }

    #[test]
    fn missing_parents_are_reported_with_line_context_in_both_report_formats() {
        for minimizers in ["", "30\t7\t"] {
            for (level, taxid) in [("D", "2"), ("R", "1")] {
                let input = format!(
                    "\n100\t4\t0\t{minimizers}{level}\t{taxid}\tParent\n100\t4\t4\t{minimizers}S\t11\t    Species\n"
                );
                for taxon_specs in [HashSet::default(), filters(&["999"])] {
                    let error = load_kreport(input.as_bytes(), taxon_specs).unwrap_err();
                    assert!(matches!(
                        error,
                        Error::Parse { line: 3, ref source }
                            if source.to_string() == "No taxon at hierarchy depth 1 in the current lineage"
                    ));
                    assert!(error.source().is_some());
                }
            }
        }
    }

    #[test]
    fn reading_after_a_lineage_error_preserves_the_last_valid_ancestors() {
        let input = b"100\t4\t0\tD\t2\tBacteria\n100\t4\t4\tS\t11\t    Missing parent\n100\t4\t0\tG\t10\t  Genus\n";
        let mut reader = KrakenReportReader::new(input.as_slice());
        let bacteria = reader.read_entry().unwrap().unwrap();

        assert!(matches!(
            reader.read_entry(),
            Err(Error::Parse { line: 2, .. })
        ));
        let genus = reader.read_entry().unwrap().unwrap();
        assert_eq!(genus.taxon().taxid().as_str(), "10");
        assert_eq!(genus.lineage(), std::slice::from_ref(bacteria.taxon()));
        assert!(reader.read_entry().unwrap().is_none());
    }

    #[test]
    fn filtered_reading_propagates_parse_errors_after_unselected_entries() {
        let input = b"100\t4\t0\tD\t2\tBacteria\n\nbroken\n";
        let mut reader = KrakenReportReader::with_filters(filters(&["999"]), input.as_slice());
        assert!(matches!(
            reader.read_entry(),
            Err(Error::Parse {
                line: 3,
                source
            }) if source.to_string() == "Invalid line with 1 fields; expected 6 or 8"
        ));
    }

    #[test]
    fn error_debug_preserves_the_taxon_level_cause() {
        for rank in ["G256", "Gx"] {
            let input = format!("100\t4\t4\t{rank}\t10\tTaxon\n");
            let mut reader = KrakenReportReader::new(input.as_bytes());
            let error = reader.read_entry().unwrap_err();
            assert!(format!("{error:?}").contains("Caused by:"));
        }
    }

    #[test]
    fn filtered_reading_propagates_io_errors_after_unselected_entries() {
        struct FailedRead;
        impl Read for FailedRead {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("failed input"))
            }
        }
        let source = Cursor::new(b"100\t4\t0\tD\t2\tBacteria\n").chain(FailedRead);
        let mut reader = KrakenReportReader::with_filters(filters(&["999"]), source);
        assert!(matches!(reader.read_entry(), Err(Error::Read {
            line: 2, source
        }) if source.to_string() == "failed input"));
    }

    #[test]
    fn loading_returns_empty_reports_for_empty_blank_and_unclassified_only_streams() {
        for input in [b"".as_slice(), b" \n", b"0\t0\t0\tU\t0\tunclassified\n"] {
            assert!(load_kreport(input, HashSet::default()).unwrap().is_empty());
        }
    }

    #[test]
    fn services_report_parse_errors_at_physical_lines() {
        for (input, expected_line, message) in [
            (
                b"100\t4\t0\tD\t2\tBacteria\n\nbroken\n".as_slice(),
                3,
                "Invalid line with 1 fields; expected 6 or 8",
            ),
            (
                b"\n0\t0\t0\tU\t0\tunclassified\nbroken\n",
                3,
                "Invalid line with 1 fields; expected 6 or 8",
            ),
            (b"100\t4\t0\t\t1\troot\n", 1, "Missing taxonomic level"),
            (
                b"\n100\t4\t0\tR\t1\troot\n \t \r\n100\t4\t0\t\t2\t  Bacteria\n",
                4,
                "Missing taxonomic level",
            ),
        ] {
            let load_error = load_kreport(input, HashSet::default()).unwrap_err();
            let mut reader = KrakenReportReader::new(input);
            let read_error = reader.entries().collect::<Result<Vec<_>, _>>().unwrap_err();
            assert_eq!(load_error.to_string(), read_error.to_string());
            for error in [load_error, read_error] {
                assert_eq!(
                    error.to_string(),
                    format!("Failed to parse Kraken report at line {expected_line}: {message}")
                );
                assert_eq!(error.source().unwrap().to_string(), message);
                let Error::Parse { line, source } = error else {
                    panic!("expected a report parsing error");
                };
                assert_eq!(line, expected_line);
                assert_eq!(source.to_string(), message);
            }
            assert_eq!(reader.offset(), expected_line);
        }
    }

    #[test]
    fn loading_propagates_io_errors_after_valid_entries() {
        struct FailedRead;
        impl Read for FailedRead {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("failed input"))
            }
        }
        let source = Cursor::new(b"100\t4\t0\tD\t2\tBacteria\n").chain(FailedRead);
        assert!(
            matches!(load_kreport(source, HashSet::default()), Err(Error::Read {
            line: 2, source
        }) if source.kind() == io::ErrorKind::Other && source.to_string() == "failed input")
        );
    }

    #[test]
    fn yields_entries_across_buffers_and_skips_blank_lines() {
        let input = b"\n100\t4\t0\tR\t1\troot\n \t\n100\t4\t2\t30\t7\tD\t2\t  Bacteria";
        for capacity in 1..=input.len() {
            let mut reader = KrakenReportReader::with_capacity(capacity, Cursor::new(input));
            let root = reader.read_entry().unwrap().unwrap();
            assert_eq!(root.taxon().taxid().as_str(), "1");
            assert_eq!(root.hierarchy_depth(), 0);
            assert!(root.lineage().is_empty());
            assert_eq!(reader.offset(), 2);
            let bacteria = reader.read_entry().unwrap().unwrap();
            assert_eq!(bacteria.hierarchy_depth(), 1);
            assert!(bacteria.lineage().is_empty());
            assert_eq!(bacteria.minimizer_count(), Some(30));
            assert_eq!(bacteria.distinct_minimizer_count(), Some(7));
            assert_eq!(reader.offset(), 4);
            assert!(reader.read_entry().unwrap().is_none());
            assert!(reader.read_entry().unwrap().is_none());
        }
    }

    #[test]
    fn crlf_lines_are_independent_of_read_boundaries() {
        let input = b"\r\nBacteria\r\nSpecies\r\r\nFinal\r";
        let expected = [b"".as_slice(), b"Bacteria", b"Species\r", b"Final\r"];
        for capacity in 1..=input.len() {
            let mut reader = LineReader::with_capacity(capacity, input.as_slice());
            let lines = std::iter::from_fn(|| reader.read_line())
                .collect::<io::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(lines, expected, "capacity {capacity}");
            assert_eq!(reader.offset(), expected.len());
        }
    }

    #[test]
    fn crlf_taxonomy_selection_is_independent_of_read_boundaries() {
        let input = b"100\t4\t0\tD\t2\tBacteria\r\n100\t4\t4\tS\t11\t  Species\r\n";
        let expected = load_kreport(input.as_slice(), filters(&["Bacteria"]))
            .unwrap()
            .into_entries();
        for capacity in 1..=input.len() {
            let mut reader = KrakenReportReader::with_capacity_and_filters(
                capacity,
                filters(&["Bacteria"]),
                input.as_slice(),
            );
            let entries = reader.entries().collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(entries, expected, "capacity {capacity}");
        }
    }

    #[test]
    fn empty_and_blank_only_inputs_reach_eof() {
        for input in [b"".as_slice(), b"\n \t\n"] {
            let mut reader = KrakenReportReader::new(Cursor::new(input));
            assert!(reader.read_entry().unwrap().is_none());
        }
    }

    #[test]
    fn parse_errors_are_typed_and_do_not_change_ancestry() {
        let mut reader = KrakenReportReader::new(Cursor::new(
            b"100\t4\t0\tD\t2\tBacteria\ninvalid\n100\t4\t2\tS\t11\t  Species\n",
        ));
        assert!(reader.read_entry().unwrap().is_some());
        assert!(matches!(
            reader.read_entry(),
            Err(Error::Parse {
                line: 2,
                source
            }) if source.to_string() == "Invalid line with 1 fields; expected 6 or 8"
        ));
        assert_eq!(reader.offset(), 2);
        let species = reader.read_entry().unwrap().unwrap();
        assert_eq!(
            species
                .lineage()
                .iter()
                .map(|taxon| taxon.taxid().as_str())
                .collect::<Vec<_>>(),
            ["2"]
        );
    }

    #[test]
    fn io_failures_identify_the_next_physical_line_after_entries() {
        struct FailedRead;
        impl Read for FailedRead {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("interrupted report input"))
            }
        }
        for (input, entry_count, expected_line) in [
            (b"100\t4\t0\tD\t2\tBacteria\n".as_slice(), 1, 2),
            (
                b"0\t0\t0\tU\t0\tunclassified\n100\t2\t0\tR\t1\troot\n100\t2\t0\tD\t2\t  Bacteria\n100\t2\t2\tS\t11\t    Species\n",
                3,
                5,
            ),
        ] {
            let source = Cursor::new(input).chain(FailedRead);
            let mut reader = KrakenReportReader::new(source);
            for _ in 0..entry_count {
                reader.read_entry().unwrap().unwrap();
            }
            let error = reader.read_entry().unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("Failed to read Kraken report at line {expected_line}: interrupted report input")
            );
            assert_eq!(
                error.source().unwrap().downcast_ref::<io::Error>().unwrap().kind(),
                io::ErrorKind::Other
            );
            assert!(matches!(error, Error::Read { line, source }
                if line == expected_line && source.to_string() == "interrupted report input"));
            assert_eq!(reader.offset(), expected_line - 1);
        }
    }

    #[test]
    fn skips_unclassified_rows_without_changing_ancestry() {
        let input = b"100\t4\t0\tD\t2\tBacteria\n20\t1\t1\tU\t0\tunclassified\n100\t4\t4\tS\t11\t  Species\n20\t1\t1\tU\t0\tunclassified";
        let mut reader = KrakenReportReader::new(Cursor::new(input));
        assert_eq!(
            reader
                .read_entry()
                .unwrap()
                .unwrap()
                .taxon()
                .taxid()
                .as_str(),
            "2"
        );
        let species = reader.read_entry().unwrap().unwrap();
        assert_eq!(species.taxon().taxid().as_str(), "11");
        assert_eq!(species.lineage().len(), 1);
        assert_eq!(species.lineage()[0].taxid().as_str(), "2");
        assert_eq!(reader.offset(), 3);
        assert!(reader.read_entry().unwrap().is_none());
        assert_eq!(reader.offset(), 4);
    }

    #[test]
    fn a_second_top_level_entry_is_rejected_without_replacing_the_lineage() {
        for minimizers in ["", "30\t7\t"] {
            let input = format!(
                "100\t4\t0\t{minimizers}R\t1\troot\n\
                 100\t4\t0\t{minimizers}D\t2\t  Bacteria\n\
                 100\t4\t0\t{minimizers}R\t1\troot\n\
                 100\t4\t4\t{minimizers}G\t10\t    Genus\n"
            );
            let mut reader = KrakenReportReader::new(input.as_bytes());
            reader.read_entry().unwrap().unwrap();
            let bacteria = reader.read_entry().unwrap().unwrap();
            assert!(matches!(
                reader.read_entry(),
                Err(Error::Parse { line: 3, ref source })
                    if source.to_string() == "Cannot descend from hierarchy depth 1 to 0; expected an immediate child"
            ));
            let genus = reader.read_entry().unwrap().unwrap();
            assert_eq!(genus.lineage(), std::slice::from_ref(bacteria.taxon()));
        }
    }

    #[test]
    fn root_rank_entries_are_returned_without_root_rank_ancestors() {
        for minimizers in ["", "30\t7\t"] {
            let input = format!(
                "100\t4\t0\t{minimizers}R\t1\troot\n\
                 100\t4\t0\t{minimizers}R1\t131567\t  cellular organisms\n\
                 100\t4\t4\t{minimizers}D\t2\t    Bacteria\n"
            );
            let mut reader = KrakenReportReader::new(input.as_bytes());
            let root = reader.read_entry().unwrap().unwrap();
            assert_eq!(root.taxon().taxid().as_str(), "1");
            assert!(root.lineage().is_empty());
            let cellular = reader.read_entry().unwrap().unwrap();
            assert_eq!(cellular.taxon().taxid().as_str(), "131567");
            assert!(!cellular.taxon().is_root());
            assert!(cellular.lineage().is_empty());
            let bacteria = reader.read_entry().unwrap().unwrap();
            assert_eq!(bacteria.taxon().taxid().as_str(), "2");
            assert!(bacteria.lineage().is_empty());
        }
    }

    #[test]
    fn filtering_an_intermediate_descendant_of_root_selects_only_that_taxon() {
        for minimizers in ["", "30\t7\t"] {
            let input = format!(
                "100\t4\t0\t{minimizers}R\t1\troot\n\
                 100\t4\t0\t{minimizers}R1\t131567\t  cellular organisms\n\
                 100\t4\t4\t{minimizers}D\t2\t    Bacteria\n"
            );
            let report = load_kreport(input.as_bytes(), filters(&["131567"])).unwrap();
            assert_eq!(
                report
                    .taxids()
                    .map(|taxid| taxid.as_str())
                    .collect::<Vec<_>>(),
                ["131567"]
            );
        }
    }

    #[test]
    fn six_and_eight_column_reports_reject_invalid_taxids_with_line_context() {
        for minimizers in ["", "30\t7\t"] {
            for taxid in [
                "", "00", "02", "0002", "taxon-A", "+2", "-2", " 2", "2 ", "２",
            ] {
                let input = format!("\n100\t4\t4\t{minimizers}G\t{taxid}\tGenus\n");
                let error = load_kreport(input.as_bytes(), HashSet::default()).unwrap_err();
                assert!(
                    error.to_string().contains("line 2") && error.to_string().contains("taxid"),
                    "{taxid:?}: {error}"
                );
                assert!(error.source().is_some());
            }
        }
    }

    #[test]
    fn loading_preserves_taxids_and_rank_depths_in_both_report_formats() {
        for minimizers in ["", "30\t7\t"] {
            for taxid in [
                "10",
                "4294967295",
                "4294967296",
                "999999999999999999999999999",
            ] {
                let input = format!("100\t4\t4\t{minimizers}G2\t{taxid}\tGenus group\n");
                let report = load_kreport(
                    input.as_bytes(),
                    [taxid]
                        .into_iter()
                        .map(TryInto::try_into)
                        .collect::<Result<_, _>>()
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(report.entries()[0].taxon().taxid().as_str(), taxid);
                assert_eq!(report.entries()[0].taxon().level().to_string(), "G2");
            }
        }
    }

    #[test]
    fn loading_preserves_line_endings_and_mixed_column_formats() {
        let input = b" \r\n100\t4\t0\tR\t1\troot\r\n100\t4\t2\t30\t7\tD\t2\t  Bacteria";
        let rows = load_kreport(input.as_slice(), HashSet::default())
            .unwrap()
            .into_entries()
            .into_iter()
            .map(|entry| entry.into_parts())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].taxon.term(), "root");
        assert_eq!(rows[0].minimizer_count, None);
        assert_eq!(rows[1].taxon.term(), "Bacteria");
        assert_eq!(rows[1].minimizer_count, Some(30));
        assert_eq!(rows[1].distinct_minimizer_count, Some(7));
        assert!(rows[1].lineage.is_empty());
    }

    #[test]
    fn loading_preserves_unterminated_final_carriage_return() {
        let input = b"100\t4\t0\tD\t2\tBacteria\r";
        let rows = load_kreport(input.as_slice(), HashSet::default())
            .unwrap()
            .into_entries()
            .into_iter()
            .map(|entry| entry.into_parts())
            .collect::<Vec<_>>();
        assert_eq!(rows[0].taxon.term(), "Bacteria\r");
    }

    #[test]
    fn loading_preserves_branch_changes() {
        let input = b"\n20\t2\t2\tU\t0\tunclassified\n100\t10\t0\tR\t1\troot\n100\t10\t0\tD\t2\t  Bacteria\n100\t10\t0\tG\t10\t    Genus A\n50\t5\t5\tS\t11\t      Species A\n \t \n50\t5\t5\tS\t12\t      Species B\n100\t10\t0\tG\t20\t    Genus B\n100\t10\t10\tS\t21\t      Species C\n100\t10\t0\tD\t3\t  Archaea\n100\t10\t10\tS\t31\t    Species D\n";
        let rows = load_kreport(input.as_slice(), HashSet::default())
            .unwrap()
            .into_entries()
            .into_iter()
            .map(|entry| entry.into_parts())
            .collect::<Vec<_>>();
        let lineages: Vec<Vec<&str>> = rows
            .iter()
            .map(|row| {
                row.lineage
                    .iter()
                    .map(|taxon| taxon.taxid().as_str())
                    .collect()
            })
            .collect();
        assert_eq!(
            lineages,
            [
                vec![],
                vec![],
                vec!["2"],
                vec!["2", "10"],
                vec!["2", "10"],
                vec!["2"],
                vec!["2", "20"],
                vec![],
                vec!["3"],
            ]
        );
        assert_eq!(
            rows.iter()
                .map(|row| row.taxon.taxid().as_str())
                .collect::<Vec<_>>(),
            ["1", "2", "10", "11", "12", "20", "21", "3", "31"]
        );
    }

    #[test]
    fn reports_reject_invalid_ranks_with_physical_line_context() {
        for (rank, message) in [
            ("Xcustom", "Invalid taxonomic rank"),
            ("G256", "Invalid rank depth"),
            ("Gx", "Invalid rank depth"),
            ("G00", "Invalid rank depth"),
            ("G02", "Invalid rank depth"),
            ("G002", "Invalid rank depth"),
        ] {
            let input = format!("\n100\t4\t4\t{rank}\t10\tCustom taxon\n");
            let mut reader = KrakenReportReader::new(input.as_bytes());
            let error = reader.read_entry().unwrap_err();
            assert!(error.to_string().contains("line 2"));
            assert!(error.to_string().contains(message), "{rank}: {error}");
            assert!(error.source().is_some());
        }
    }
}
