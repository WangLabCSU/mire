use std::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::str::{self, Utf8Error};

use super::vo::{
    KrakenReportEntry, Taxid, TaxidParseError, Taxon, TaxonLevel, TaxonLevelParseError,
};

/// A path through consecutive levels of the taxonomic hierarchy.
pub(crate) struct LineagePath {
    // The endpoint's report depth; None exactly when the path is empty.
    depth: Option<usize>,
    // Taxa from the first known ancestor through the endpoint, at consecutive depths.
    lineage: Vec<Taxon>,
}

impl LineagePath {
    /// Start an empty taxonomic path.
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self::with_capacity(0)
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            depth: None,
            lineage: Vec::with_capacity(capacity),
        }
    }

    /// Taxa from the first known ancestor through the current endpoint.
    fn lineage(&self) -> &Vec<Taxon> {
        &self.lineage
    }

    /// Shorten the path to end at the taxon at the requested hierarchy depth.
    ///
    /// # Errors
    /// Returns an error if the requested depth is not on the path, leaving it unchanged.
    fn ascend(&mut self, target_depth: usize) -> Result<(), LineagePathError> {
        // Validate before updating either field so a rejected ascent preserves the path.
        // A path may start below the root, so report depths are not vector indices.
        // The depth difference counts the taxa to remove after the target ancestor.
        let steps_back = self
            .depth
            .and_then(|depth| depth.checked_sub(target_depth))
            // Retain the target taxon; removing every taxon would accept an unknown ancestor.
            .filter(|steps| *steps < self.lineage.len())
            .ok_or(LineagePathError::UnknownDepth {
                depth: target_depth,
            })?;
        self.lineage.truncate(self.lineage.len() - steps_back);
        self.depth = Some(target_depth);
        Ok(())
    }

    /// Extend the path to a direct child of its current endpoint.
    /// An empty path can start with a taxon at any hierarchy depth.
    ///
    /// # Errors
    /// Returns an error if the new depth is not immediately below the endpoint,
    /// leaving the path unchanged.
    fn descend(&mut self, taxon: Taxon, hierarchy_depth: usize) -> Result<(), LineagePathError> {
        // Validate the new taxon's parent before changing lineage or depth,
        // so an invalid descent leaves the existing path unchanged.
        // A taxon at hierarchy depth 0 is allowed only when the path is empty
        // (self.depth is None); it cannot be a child of an existing endpoint.
        if let Some(current_depth) = self.depth {
            // Compare parent depths to reject another top-level taxon without
            // adding to current_depth, which could overflow at usize::MAX.
            if hierarchy_depth.checked_sub(1) != Some(current_depth) {
                return Err(LineagePathError::InvalidDescent {
                    current_depth,
                    target_depth: hierarchy_depth,
                });
            }
        }
        self.lineage.push(taxon);
        self.depth = Some(hierarchy_depth);
        Ok(())
    }
}

/// A taxonomic lineage path could not be traversed.
#[derive(Debug, thiserror::Error)]
enum LineagePathError {
    /// The requested ancestor is not on the path.
    #[error("No taxon at hierarchy depth {depth} in the current lineage")]
    UnknownDepth { depth: usize },

    /// The new taxon is not an immediate child of the path's endpoint.
    #[error("Cannot descend from hierarchy depth {current_depth} to {target_depth}; expected an immediate child")]
    InvalidDescent {
        current_depth: usize,
        target_depth: usize,
    },
}

// Interpret report rows, coordinate path movement and assemble each entry's ancestors.
pub(crate) struct KrakenReportParser;

impl KrakenReportParser {
    pub(crate) fn new() -> Self {
        KrakenReportParser
    }

    pub(crate) fn parse_entry(
        &self,
        line: &[u8],
        path: &mut LineagePath,
    ) -> Result<KrakenReportEntry, ParseError> {
        // Blank lines carry no hierarchy information and must not alter the path.
        if line.iter().all(|byte| byte.is_ascii_whitespace()) {
            return Err(ParseError::EmptyLine);
        }

        // Split on tabs to preserve empty fields and the scientific name's
        // indentation; splitting on whitespace would lose both.
        let fields = line.split(|byte| *byte == b'\t').collect::<Vec<_>>();

        // https://github.com/DerrickWood/kraken2/blob/master/docs/MANUAL.markdown
        // 1. Percentage of fragments covered by the clade rooted at this taxon
        // 2. Number of fragments covered by the clade rooted at this taxon
        // 3. Number of fragments assigned directly to this taxon
        // * 4. Number of minimizers in read data associated with this taxon (new)
        // * 5. An estimate of the number of distinct minimizers in read data
        //    associated with this taxon (new)
        // 6. A taxonomic level starts with a major rank abbreviation:
        //    (U)nclassified, (R)oot, (D)omain,
        //    (K)ingdom, (P)hylum, (C)lass, (O)rder, (F)amily, (G)enus or (S)pecies.
        //    Intermediate ranks append a depth indicating the distance from the
        //    nearest ancestor at a major rank. For example, "G2" denotes a taxon
        //    two levels below its genus ancestor.
        // 7. NCBI taxonomic ID number
        // 8. Indented scientific name
        //
        // Map the six- and eight-column layouts to the same fields. The
        // six-column layout has no minimizer statistics.
        let (
            percentage,
            clade_reads,
            direct_reads,
            minimizer_count,
            distinct_minimizer_count,
            level,
            taxid,
            indented_term,
        ) = match fields.as_slice() {
            [percentage, clade_reads, direct_reads, level, taxid, taxon] => (
                *percentage,
                *clade_reads,
                *direct_reads,
                None,
                None,
                *level,
                *taxid,
                *taxon,
            ),
            [percentage, clade_reads, direct_reads, minimizer_count, distinct_minimizer_count, level, taxid, taxon] => {
                (
                    *percentage,
                    *clade_reads,
                    *direct_reads,
                    Some(*minimizer_count),
                    Some(*distinct_minimizer_count),
                    *level,
                    *taxid,
                    *taxon,
                )
            }
            _ => {
                return Err(EntryFailure::InvalidFieldCount {
                    actual: fields.len(),
                }
                .into());
            }
        };

        level.first().ok_or(EntryFailure::MissingLevel)?;
        let level = Self::parse_text(level, "taxonomic level")?;

        if taxid.is_empty() {
            return Err(EntryFailure::MissingTaxid.into());
        }
        let taxid = Self::parse_text(taxid, "taxid")?;

        // Each pair of leading spaces is one step in the report hierarchy.
        // Unlike the depth in G2, this depth is measured from the report root.
        let indentation = indented_term
            .iter()
            .take_while(|byte| **byte == b' ')
            .count();
        if indentation % 2 != 0 {
            return Err(EntryFailure::InvalidTaxonIndentation.into());
        }
        let hierarchy_depth = indentation / 2;

        // Remove structural indentation without trimming the scientific name.
        let term = &indented_term[indentation..];
        if term.is_empty() {
            return Err(EntryFailure::MissingTaxonName.into());
        }
        let term = Self::parse_text(term, "taxonomic name")?;

        // Validate the taxonomic level and taxid before changing ancestry.
        let taxon = Taxon::new(
            TaxonLevel::parse(level)?,
            Taxid::new(taxid.to_owned())?,
            term.to_owned(),
        );
        let lineage;
        if !taxon.is_unclassified() {
            // Update ancestry before parsing statistics: a rejected numeric field
            // still leaves a known parent for subsequent rows. For example, a
            // species following a genus with an invalid count belongs to that
            // genus, not to the previous branch. Do not roll back this path later.
            // Keep the incoming taxon's parent, dropping the previous branch
            // below it. A depth-zero row has no parent; descend validates it.
            if let Some(parent_depth) = hierarchy_depth.checked_sub(1) {
                path.ascend(parent_depth)?;
            }

            // Build the entry's ancestors between ascent and descent so the
            // current taxon is not included. Keep the full path for later ascents,
            // but skip root-rank ancestors (R, R1, R2, ...) before cloning.
            lineage = Some(
                path.lineage()
                    .iter()
                    .skip_while(|taxon| taxon.is_root_rank())
                    .cloned()
                    .collect(),
            );
            path.descend(taxon.clone(), hierarchy_depth)?;
        } else {
            // Leave the path unchanged, but still validate statistics before
            // reporting an unclassified row so malformed fields remain errors.
            lineage = None;
        }

        // Missing minimizer columns remain None; malformed values in present
        // columns must still fail parsing.
        let minimizer_count = minimizer_count
            .map(|value| Self::parse_usize(value, "minimizer count"))
            .transpose()?;
        let distinct_minimizer_count = distinct_minimizer_count
            .map(|value| Self::parse_usize(value, "distinct minimizer count"))
            .transpose()?;

        let percentage = Self::parse_float(percentage, "percentage")?;
        let clade_reads = Self::parse_usize(clade_reads, "clade reads")?;
        let direct_reads = Self::parse_usize(direct_reads, "direct reads")?;

        // Report an unclassified row only after all its fields are validated.
        let lineage = lineage.ok_or(ParseError::Unclassified)?;
        Ok(KrakenReportEntry::new(
            percentage,
            clade_reads,
            direct_reads,
            minimizer_count,
            distinct_minimizer_count,
            taxon,
            lineage,
        ))
    }

    fn parse_text<'a>(value: &'a [u8], field: &'static str) -> Result<&'a str, EntryFailure> {
        str::from_utf8(value).map_err(|source| EntryFailure::InvalidUtf8 { field, source })
    }

    fn parse_float(value: &[u8], field: &'static str) -> Result<f64, EntryFailure> {
        let value = str::from_utf8(value.trim_ascii())
            .map_err(|source| EntryFailure::InvalidUtf8 { field, source })?;

        value.parse().map_err(|source| EntryFailure::InvalidFloat {
            field,
            value: value.to_owned(),
            source,
        })
    }

    fn parse_usize(value: &[u8], field: &'static str) -> Result<usize, EntryFailure> {
        let value = str::from_utf8(value.trim_ascii())
            .map_err(|source| EntryFailure::InvalidUtf8 { field, source })?;

        value
            .parse()
            .map_err(|source| EntryFailure::InvalidInteger {
                field,
                value: value.to_owned(),
                source,
            })
    }
}

/// A report entry could not be parsed.
///
/// Match the variant to distinguish a blank line, a valid unclassified row,
/// or an invalid report entry. Display the error for diagnostic details and use
/// [`std::error::Error::source()`] to inspect its underlying cause, when available.
///
/// ```
/// use kreport::{Error, InvalidEntryError, KrakenReportReader, ParseError};
///
/// let mut reader = KrakenReportReader::new(b"broken\n".as_slice());
/// match reader.read_entry() {
///     Err(Error::Parse { line, source: ParseError::InvalidEntry(error) }) => {
///         let error: InvalidEntryError = error;
///         assert_eq!(line, 1);
///         assert_eq!(error.to_string(), "Invalid line with 1 fields; expected 6 or 8");
///     }
///     result => panic!("expected an invalid entry, got {result:?}"),
/// }
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// The report line is blank.
    #[error("Empty report line")]
    EmptyLine,

    /// The row describes unclassified reads and has valid report fields.
    #[error("Report row describes unclassified reads")]
    Unclassified,

    /// The report entry is invalid.
    #[error(transparent)]
    InvalidEntry(#[from] InvalidEntryError),
}

/// Details of an invalid Kraken report entry.
///
/// Display this error for the failure message. Use
/// [`std::error::Error::source()`] to inspect the underlying cause, when available.
#[derive(thiserror::Error)]
#[error(transparent)]
pub struct InvalidEntryError(EntryFailure);

impl fmt::Debug for InvalidEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl From<EntryFailure> for ParseError {
    fn from(error: EntryFailure) -> Self {
        Self::InvalidEntry(InvalidEntryError(error))
    }
}

impl From<TaxonLevelParseError> for ParseError {
    fn from(error: TaxonLevelParseError) -> Self {
        EntryFailure::from(error).into()
    }
}

impl From<TaxidParseError> for ParseError {
    fn from(error: TaxidParseError) -> Self {
        EntryFailure::from(error).into()
    }
}

impl From<LineagePathError> for ParseError {
    fn from(error: LineagePathError) -> Self {
        EntryFailure::from(error).into()
    }
}

// Keep detailed failures typed without exposing them as public classifications.
#[derive(Debug, thiserror::Error)]
enum EntryFailure {
    #[error("Invalid line with {actual} fields; expected 6 or 8")]
    InvalidFieldCount { actual: usize },

    #[error("Missing taxonomic level")]
    MissingLevel,

    #[error(transparent)]
    InvalidTaxonLevel(#[from] TaxonLevelParseError),

    #[error("Missing taxid")]
    MissingTaxid,

    #[error(transparent)]
    InvalidTaxid(#[from] TaxidParseError),

    #[error("Missing taxon name")]
    MissingTaxonName,

    #[error("Invalid taxon indentation; expected two spaces per taxonomic level")]
    InvalidTaxonIndentation,

    #[error(transparent)]
    InvalidLineagePath(#[from] LineagePathError),

    #[error("Invalid UTF-8 in {field}")]
    InvalidUtf8 {
        field: &'static str,
        #[source]
        source: Utf8Error,
    },

    #[error("Invalid floating-point value '{value}' in {field}")]
    InvalidFloat {
        field: &'static str,
        value: String,
        #[source]
        source: ParseFloatError,
    },

    #[error("Invalid integer value '{value}' in {field}")]
    InvalidInteger {
        field: &'static str,
        value: String,
        #[source]
        source: ParseIntError,
    },
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::{
        EntryFailure, InvalidEntryError, KrakenReportEntry, KrakenReportParser, LineagePath,
        LineagePathError, ParseError, Taxid, Taxon, TaxonLevel,
    };

    fn parse(contents: &[u8]) -> Result<Vec<KrakenReportEntry>, ParseError> {
        let parser = KrakenReportParser::new();
        let mut path = LineagePath::new();
        contents
            .strip_suffix(b"\n")
            .unwrap_or(contents)
            .split(|byte| *byte == b'\n')
            .map(|line| parser.parse_entry(line, &mut path))
            .collect()
    }

    #[test]
    fn classified_rows_preserve_report_order() {
        let entries = parse(
            b"100.00\t10\t0\tR\t1\troot\n\
             100.00\t10\t1\tD\t2\t  Bacteria\n\
             50.00\t5\t5\tS\t562\t    Escherichia coli\n",
        )
        .unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].taxon().level().to_string(), "R");
        assert_eq!(entries[0].taxon().taxid().as_str(), "1");
        assert_eq!(entries[0].taxon().term(), "root");
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.taxon().taxid().as_str())
                .collect::<Vec<_>>(),
            ["1", "2", "562"]
        );
        assert_eq!(entries[2].taxon().term(), "Escherichia coli");
    }

    #[test]
    fn unclassified_rows_return_unclassified_errors() {
        let parser = KrakenReportParser::new();
        let mut path = LineagePath::new();
        for minimizers in ["", "20\t5\t"] {
            for level in ["U", "U1"] {
                let line = format!("20\t1\t1\t{minimizers}{level}\t0\tunclassified");
                assert!(matches!(
                    parser.parse_entry(line.as_bytes(), &mut path),
                    Err(ParseError::Unclassified)
                ));
            }
        }
    }

    #[test]
    fn unclassified_rows_reject_invalid_statistics() {
        for (statistics, field) in [
            ("invalid\t1\t1", "percentage"),
            ("20\tinvalid\t1", "clade reads"),
            ("20\t1\tinvalid", "direct reads"),
            ("invalid\t1\t1\t20\t5", "percentage"),
            ("20\tinvalid\t1\t20\t5", "clade reads"),
            ("20\t1\tinvalid\t20\t5", "direct reads"),
            ("20\t1\t1\tinvalid\t5", "minimizer count"),
            ("20\t1\t1\t20\tinvalid", "distinct minimizer count"),
        ] {
            let input = format!("{statistics}\tU\t0\tunclassified");
            assert!(matches!(
                parse(input.as_bytes()),
                Err(ParseError::InvalidEntry(InvalidEntryError(EntryFailure::InvalidFloat { field: actual, .. }))
                    | ParseError::InvalidEntry(InvalidEntryError(EntryFailure::InvalidInteger { field: actual, .. })))
                    if actual == field
            ));
        }
    }

    #[test]
    fn unclassified_rows_reject_invalid_taxids() {
        for minimizers in ["", "20\t5\t"] {
            let invalid_taxid = format!("20\t1\t1\t{minimizers}U\t00\tunclassified");
            assert!(matches!(
                parse(invalid_taxid.as_bytes()),
                Err(ParseError::InvalidEntry(InvalidEntryError(
                    EntryFailure::InvalidTaxid(_)
                )))
            ));
        }
    }

    #[test]
    fn preserves_fragment_statistics() {
        for minimizers in ["", "400\t40\t"] {
            let input = format!("62.5\t25\t7\t{minimizers}G\t10\tGenus\n");
            let entries = parse(input.as_bytes()).unwrap();
            let parts = entries.into_iter().next().unwrap().into_parts();
            assert_eq!(parts.percentage, 62.5);
            assert_eq!(parts.clade_reads, 25);
            assert_eq!(parts.direct_reads, 7);
        }
    }

    #[test]
    fn preserves_optional_values_in_mixed_report_formats() {
        let entries = parse(
            b"100.00\t10\t0\tR\t1\troot\n\
             100.00\t10\t1\t400\t40\tD\t2\t  Bacteria\n",
        )
        .unwrap();
        assert_eq!(entries[0].minimizer_count(), None);
        assert_eq!(entries[0].distinct_minimizer_count(), None);
        assert_eq!(entries[1].minimizer_count(), Some(400));
        assert_eq!(entries[1].distinct_minimizer_count(), Some(40));
    }

    #[test]
    fn parsed_entries_include_ancestor_lineages() {
        for minimizers in ["", "20\t5\t"] {
            let input = format!(
                "100\t4\t0\t{minimizers}R\t1\troot\n\
                 100\t4\t0\t{minimizers}D\t2\t  Bacteria\n\
                 100\t4\t0\t{minimizers}G2\t10\t    Genus\n\
                 50\t2\t2\t{minimizers}S\t11\t      Species A\n\
                 50\t2\t2\t{minimizers}S\t12\t      Species B\n\
                 100\t4\t0\t{minimizers}D\t3\t  Archaea\n"
            );
            let entries = parse(input.as_bytes()).unwrap();
            assert_eq!(
                entries
                    .iter()
                    .map(|entry| entry
                        .lineage()
                        .iter()
                        .map(|taxon| taxon.taxid().as_str())
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                [
                    vec![],
                    vec![],
                    vec!["2"],
                    vec!["2", "10"],
                    vec!["2", "10"],
                    vec![]
                ]
            );
        }
    }

    #[test]
    fn blank_unclassified_and_invalid_taxonomy_lines_preserve_lineage() {
        let parser = KrakenReportParser::new();
        for line in [
            b" \t\r\n".as_slice(),
            b"20\t1\t1\tU\t0\tunclassified",
            b"broken",
            b"100\t4\t0\tG2\t02\t  Genus",
        ] {
            let mut path = LineagePath::new();
            parser
                .parse_entry(b"100\t4\t0\tD\t2\tBacteria", &mut path)
                .unwrap();
            assert!(parser.parse_entry(line, &mut path).is_err());
            let species = parser
                .parse_entry(b"100\t4\t4\tS\t11\t  Species", &mut path)
                .unwrap();
            assert_eq!(
                species
                    .lineage()
                    .iter()
                    .map(|taxon| taxon.taxid().as_str())
                    .collect::<Vec<_>>(),
                ["2"],
                "{line:?}"
            );
        }
    }

    #[test]
    fn invalid_fragment_statistics_preserve_the_new_parent_for_descendants() {
        let parser = KrakenReportParser::new();
        for minimizers in ["", "20\t5\t"] {
            for statistics in ["invalid\t4\t0", "100\tinvalid\t0", "100\t4\tinvalid"] {
                let mut path = LineagePath::new();
                parser
                    .parse_entry(b"100\t4\t0\tD\t2\tBacteria", &mut path)
                    .unwrap();
                parser
                    .parse_entry(b"100\t4\t0\tG\t10\t  Genus A", &mut path)
                    .unwrap();
                let parent = format!("{statistics}\t{minimizers}G\t20\t  Genus B");
                assert!(parser.parse_entry(parent.as_bytes(), &mut path).is_err());

                let species = parser
                    .parse_entry(b"100\t4\t4\tS\t21\t    Species B", &mut path)
                    .unwrap();
                assert_eq!(
                    species
                        .lineage()
                        .iter()
                        .map(|taxon| taxon.taxid().as_str())
                        .collect::<Vec<_>>(),
                    ["2", "20"],
                    "{parent}"
                );
            }
        }
    }

    #[test]
    fn blank_lines_return_empty_line_errors() {
        let parser = KrakenReportParser::new();
        let mut path = LineagePath::new();
        for line in [b"".as_slice(), b" ", b"\t", b" \t\r\n\x0c"] {
            assert!(matches!(
                parser.parse_entry(line, &mut path),
                Err(ParseError::EmptyLine)
            ));
        }
    }

    #[test]
    fn nonblank_malformed_lines_are_errors() {
        let parser = KrakenReportParser::new();
        let mut path = LineagePath::new();
        for line in [b"broken".as_slice(), b" \tbroken\r", b"\0", b"\xc2\xa0"] {
            assert!(matches!(
                parser.parse_entry(line, &mut path),
                Err(ParseError::InvalidEntry(InvalidEntryError(
                    EntryFailure::InvalidFieldCount { .. }
                )))
            ));
        }
    }

    #[test]
    fn rejects_missing_level() {
        let error = parse(b"100.00\t10\t0\t\t1\troot\n").expect_err("expected an error");

        assert!(matches!(
            error,
            ParseError::InvalidEntry(InvalidEntryError(EntryFailure::MissingLevel))
        ));
    }

    #[test]
    fn rejects_odd_taxon_indentation() {
        let error = parse(b"100.00\t10\t1\tD\t2\t Bacteria\n").expect_err("expected an error");

        assert!(matches!(
            error,
            ParseError::InvalidEntry(InvalidEntryError(EntryFailure::InvalidTaxonIndentation))
        ));
    }

    #[test]
    fn reports_the_invalid_numeric_field() {
        let error = parse(b"invalid\t10\t0\tR\t1\troot\n").expect_err("expected an error");

        assert!(matches!(
            error,
            ParseError::InvalidEntry(InvalidEntryError(EntryFailure::InvalidFloat {
                field: "percentage",
                ..
            }))
        ));
    }

    #[test]
    fn rejects_invalid_utf8_in_domain_text() {
        let contents = b"100.00\t10\t1\tD\t2\t  \xFF\n";
        let error = parse(contents).expect_err("expected an error");

        assert!(matches!(
            error,
            ParseError::InvalidEntry(InvalidEntryError(EntryFailure::InvalidUtf8 {
                field: "taxonomic name",
                ..
            }))
        ));
    }

    fn taxon(level: &str, taxid: &str) -> Taxon {
        Taxon::new(
            TaxonLevel::parse(level).unwrap(),
            Taxid::new(taxid.to_owned()).unwrap(),
            taxid.to_owned(),
        )
    }

    #[test]
    fn lineages_contain_only_ancestors_in_report_order() {
        let entries = parse(
            b"100\t4\t0\tD\t2\tBacteria\n100\t4\t0\tG\t10\t  Genus\n100\t4\t4\tS\t11\t    Species\n",
        )
        .unwrap();

        assert!(entries[0].lineage().is_empty());
        assert_eq!(entries[1].lineage(), slice::from_ref(entries[0].taxon()));
        assert_eq!(
            entries[2].lineage(),
            [entries[0].taxon().clone(), entries[1].taxon().clone()]
        );
    }

    #[test]
    fn empty_lineages_have_no_depth() {
        for path in [LineagePath::new(), LineagePath::with_capacity(10)] {
            assert!(path.lineage.is_empty());
            assert_eq!(path.depth, None);
        }
    }

    #[test]
    fn ascending_to_an_unknown_depth_preserves_the_lineage() {
        for target_depth in [0, 2, 5, usize::MAX] {
            let mut path = LineagePath::new();
            let genus = taxon("G", "10");
            let species = taxon("S", "11");
            path.descend(genus.clone(), 3).unwrap();
            path.descend(species.clone(), 4).unwrap();

            assert!(matches!(
                path.ascend(target_depth),
                Err(LineagePathError::UnknownDepth { depth }) if depth == target_depth
            ));
            assert_eq!(path.lineage, [genus, species]);
            assert_eq!(path.depth, Some(4));
        }
    }

    #[test]
    fn ascending_an_empty_lineage_returns_an_error() {
        for depth in [0, 1, usize::MAX] {
            let mut path = LineagePath::new();
            assert!(matches!(
                path.ascend(depth),
                Err(LineagePathError::UnknownDepth { .. })
            ));
            assert!(path.lineage.is_empty());
            assert_eq!(path.depth, None);
        }
    }

    #[test]
    fn descending_to_a_nonchild_depth_preserves_the_lineage() {
        for target_depth in [0, 2, 3, 5, usize::MAX] {
            let mut path = LineagePath::new();
            let genus = taxon("G", "10");
            path.descend(genus.clone(), 3).unwrap();

            assert!(matches!(
                path.descend(taxon("S", "11"), target_depth),
                Err(LineagePathError::InvalidDescent {
                    current_depth: 3,
                    target_depth: actual,
                }) if actual == target_depth
            ));
            assert_eq!(path.lineage, [genus]);
            assert_eq!(path.depth, Some(3));
        }
    }

    #[test]
    fn descending_beyond_the_maximum_depth_returns_an_error() {
        let mut path = LineagePath::new();
        let genus = taxon("G", "10");
        path.descend(genus.clone(), usize::MAX).unwrap();

        for depth in [0, usize::MAX] {
            assert!(matches!(
                path.descend(taxon("S", "11"), depth),
                Err(LineagePathError::InvalidDescent { .. })
            ));
            assert_eq!(path.lineage, slice::from_ref(&genus));
            assert_eq!(path.depth, Some(usize::MAX));
        }
    }

    #[test]
    fn ascending_retains_the_taxon_at_the_target_depth() {
        let lineage = [taxon("D", "2"), taxon("G", "10"), taxon("S", "11")];

        for target_depth in 0..lineage.len() {
            let mut path = LineagePath::new();
            for (depth, taxon) in lineage.iter().enumerate() {
                path.descend(taxon.clone(), depth).unwrap();
            }

            path.ascend(target_depth).unwrap();
            assert_eq!(path.lineage, lineage[..=target_depth]);
            assert_eq!(path.depth, Some(target_depth));
        }
    }

    #[test]
    fn siblings_share_ancestors() {
        let mut path = LineagePath::new();
        let genus = taxon("G", "10");
        path.descend(genus.clone(), 0).unwrap();

        path.descend(taxon("S", "11"), 1).unwrap();
        path.ascend(0).unwrap();
        assert_eq!(path.lineage, slice::from_ref(&genus));
        path.descend(taxon("S", "12"), 1).unwrap();
        path.ascend(0).unwrap();
        assert_eq!(path.lineage, [genus]);
    }

    #[test]
    fn siblings_below_a_missing_parent_share_the_first_known_ancestor() {
        let mut path = LineagePath::new();
        let genus = taxon("G", "10");
        path.descend(genus.clone(), 3).unwrap();
        path.descend(taxon("S", "11"), 4).unwrap();

        path.ascend(3).unwrap();
        assert_eq!(path.lineage, [genus]);
    }

    #[test]
    fn changing_branches_replaces_ancestors_below_the_shared_parent() {
        let mut path = LineagePath::new();
        let bacteria = taxon("D", "2");
        let next_genus = taxon("G", "20");
        path.descend(bacteria.clone(), 0).unwrap();
        path.descend(taxon("G", "10"), 1).unwrap();
        path.descend(taxon("S", "11"), 2).unwrap();

        path.ascend(0).unwrap();
        assert_eq!(path.lineage, slice::from_ref(&bacteria));
        path.descend(next_genus.clone(), 1).unwrap();
        path.ascend(1).unwrap();
        assert_eq!(path.lineage, [bacteria, next_genus]);
    }

    #[test]
    fn a_second_top_level_taxon_returns_a_parse_error_without_changing_lineage() {
        let parser = KrakenReportParser::new();
        for current_depth in [0, 1] {
            for (level, taxid) in [("R", "1"), ("D", "3")] {
                let mut path = LineagePath::new();
                parser
                    .parse_entry(b"100\t4\t0\tR\t1\troot", &mut path)
                    .unwrap();
                if current_depth == 1 {
                    parser
                        .parse_entry(b"100\t4\t0\tD\t2\t  Bacteria", &mut path)
                        .unwrap();
                }
                let lineage = path.lineage.clone();
                let line = format!("100\t4\t0\t{level}\t{taxid}\tAnother top-level taxon");
                assert!(matches!(
                    parser.parse_entry(line.as_bytes(), &mut path),
                    Err(ParseError::InvalidEntry(InvalidEntryError(EntryFailure::InvalidLineagePath(
                        LineagePathError::InvalidDescent { current_depth: actual, target_depth: 0 }
                    )))) if actual == current_depth
                ));
                assert_eq!(path.lineage, lineage);
                assert_eq!(path.depth, Some(current_depth));
            }
        }
    }

    #[test]
    fn a_missing_immediate_parent_returns_a_parse_error() {
        for (parent_level, taxid) in [("D", "2"), ("R", "1")] {
            let parser = KrakenReportParser::new();
            let mut path = LineagePath::new();
            let parent = format!("100\t4\t0\t{parent_level}\t{taxid}\tParent");
            let entry = parser.parse_entry(parent.as_bytes(), &mut path).unwrap();

            assert!(matches!(
                parser.parse_entry(b"100\t4\t4\tG\t10\t    Genus", &mut path),
                Err(ParseError::InvalidEntry(InvalidEntryError(
                    EntryFailure::InvalidLineagePath(LineagePathError::UnknownDepth { depth: 1 })
                )))
            ));
            assert_eq!(path.lineage, slice::from_ref(entry.taxon()));
            assert_eq!(path.depth, Some(0));
        }
    }

    #[test]
    fn the_first_taxon_can_start_below_the_top_level() {
        for hierarchy_depth in [1, 3, usize::MAX - 1] {
            let mut path = LineagePath::new();
            let genus = taxon("G", "10");

            path.descend(genus.clone(), hierarchy_depth).unwrap();
            path.ascend(hierarchy_depth).unwrap();
            assert_eq!(path.lineage, [genus]);
        }
    }

    #[test]
    fn returning_above_the_first_known_ancestor_returns_a_parse_error() {
        let parser = KrakenReportParser::new();
        let mut path = LineagePath::new();
        path.descend(taxon("G", "10"), 3).unwrap();
        path.descend(taxon("S", "11"), 4).unwrap();

        assert!(matches!(
            parser.parse_entry(b"100\t4\t0\tD\t2\t  Bacteria", &mut path),
            Err(ParseError::InvalidEntry(InvalidEntryError(
                EntryFailure::InvalidLineagePath(LineagePathError::UnknownDepth { depth: 0 })
            )))
        ));
        assert_eq!(path.lineage, [taxon("G", "10"), taxon("S", "11")]);
        assert_eq!(path.depth, Some(4));
    }

    #[test]
    fn an_indented_first_entry_returns_a_parse_error_without_changing_lineage() {
        let parser = KrakenReportParser::new();
        for hierarchy_depth in [1, 3] {
            let mut path = LineagePath::new();
            let line = format!("100\t4\t4\tG\t10\t{}Genus", "  ".repeat(hierarchy_depth));
            assert!(matches!(
                parser.parse_entry(line.as_bytes(), &mut path),
                Err(ParseError::InvalidEntry(InvalidEntryError(EntryFailure::InvalidLineagePath(
                    LineagePathError::UnknownDepth { depth }
                )))) if depth == hierarchy_depth - 1
            ));
            assert!(path.lineage.is_empty());
            assert_eq!(path.depth, None);
        }
    }

    #[test]
    fn root_taxa_are_excluded_from_descendant_lineages() {
        for root_level in ["R", "R0"] {
            let input = format!(
                "100\t4\t0\t{root_level}\t1\troot\n100\t4\t0\tD\t2\t  Bacteria\n100\t4\t4\tG\t10\t    Genus\n"
            );
            let entries = parse(input.as_bytes()).unwrap();
            assert!(entries[0].lineage().is_empty());
            assert!(entries[1].lineage().is_empty());
            assert_eq!(entries[2].lineage(), slice::from_ref(entries[1].taxon()));
        }
    }

    #[test]
    fn root_rank_ancestors_are_excluded_from_lineages() {
        for minimizers in ["", "20\t5\t"] {
            let input = format!(
                "100\t4\t0\t{minimizers}R\t1\troot\n\
                 100\t4\t0\t{minimizers}R1\t131567\t  cellular organisms\n\
                 100\t4\t0\t{minimizers}R2\t10\t    Group\n\
                 100\t4\t0\t{minimizers}D\t2\t      Bacteria\n\
                 100\t4\t0\t{minimizers}G2\t20\t        Genus group\n\
                 100\t4\t4\t{minimizers}S\t21\t          Species\n"
            );
            let entries = parse(input.as_bytes()).unwrap();
            assert_eq!(entries.len(), 6);
            assert!(entries[..4].iter().all(|entry| entry.lineage().is_empty()));
            assert_eq!(entries[4].lineage(), slice::from_ref(entries[3].taxon()));
            assert_eq!(
                entries[5].lineage(),
                [entries[3].taxon().clone(), entries[4].taxon().clone()]
            );
        }
    }

    #[test]
    fn root_rank_ancestors_remain_available_for_path_traversal() {
        let parser = KrakenReportParser::new();
        let mut path = LineagePath::new();
        let entries = [
            "100\t4\t0\tR\t1\troot",
            "100\t4\t0\tR1\t131567\t  cellular organisms",
            "100\t4\t0\tR2\t10\t    Group",
            "100\t4\t4\tD\t2\t      Bacteria",
        ]
        .map(|line| parser.parse_entry(line.as_bytes(), &mut path).unwrap());
        assert_eq!(
            path.lineage(),
            &entries
                .iter()
                .map(|entry| entry.taxon().clone())
                .collect::<Vec<_>>()
        );

        let archaea = parser
            .parse_entry(b"100\t4\t4\tD\t3\t    Archaea", &mut path)
            .unwrap();
        assert!(archaea.lineage().is_empty());
        assert_eq!(path.depth, Some(2));
        assert_eq!(
            path.lineage(),
            &[
                entries[0].taxon().clone(),
                entries[1].taxon().clone(),
                archaea.taxon().clone()
            ]
        );
    }

    #[test]
    fn report_hierarchy_is_independent_of_intermediate_rank_depth() {
        let mut path = LineagePath::new();
        let bacteria = taxon("D", "2");
        let genus = taxon("G2", "10");
        path.descend(bacteria.clone(), 0).unwrap();

        path.ascend(0).unwrap();
        assert_eq!(path.lineage, slice::from_ref(&bacteria));
        path.descend(genus.clone(), 1).unwrap();
        path.ascend(1).unwrap();
        assert_eq!(path.lineage, [bacteria, genus]);
    }
}
