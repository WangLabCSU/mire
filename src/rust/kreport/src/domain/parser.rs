use std::fmt;
use std::num::{ParseFloatError, ParseIntError};
use std::str::{self, Utf8Error};

use super::vo::{
    KrakenReportEntry, Taxid, TaxidParseError, Taxon, TaxonLevel, TaxonLevelParseError,
};

/// Maintain the taxonomic lineage while parsing a Kraken report.
pub(crate) struct LineageState {
    hierarchy_depths: Vec<usize>,
    lineage: Vec<Taxon>,
}

impl LineageState {
    /// Start a report with no known ancestors.
    pub(crate) fn new() -> Self {
        Self::with_capacity(10)
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            hierarchy_depths: Vec::with_capacity(capacity),
            lineage: Vec::with_capacity(capacity),
        }
    }

    /// Advance the lineage state and return the taxon's ancestor lineage.
    // Hierarchy depth identifies the parent in the report. It is independent
    // of intermediate rank depth, such as the distance below genus in `G2`.
    // If the immediate parent is absent, the taxon starts a new lineage.
    fn advance(&mut self, taxon: &Taxon, hierarchy_depth: usize) -> Vec<Taxon> {
        while let Some(ancestor_depth) = self.hierarchy_depths.last() {
            if Some(*ancestor_depth) == hierarchy_depth.checked_sub(1) {
                break;
            }
            self.hierarchy_depths.pop();
            self.lineage.pop();
        }
        let lineage = self.lineage.clone();
        if !taxon.is_root() {
            self.hierarchy_depths.push(hierarchy_depth);
            self.lineage.push(taxon.clone());
        }
        lineage
    }
}

// Parse report rows and advance the lineage state.
pub(crate) struct KrakenReportParser;

impl KrakenReportParser {
    pub(crate) fn parse_entry(
        line: &[u8],
        state: &mut LineageState,
    ) -> Result<Option<KrakenReportEntry>, ParseError> {
        if line.iter().all(|byte| byte.is_ascii_whitespace()) {
            return Ok(None);
        }
        Self::parse_line(line, state).map_err(ParseError)
    }

    fn parse_line(
        line: &[u8],
        state: &mut LineageState,
    ) -> Result<Option<KrakenReportEntry>, ParseErrorKind> {
        let fields = line.split(|byte| *byte == b'\t').collect::<Vec<_>>();

        // https://github.com/DerrickWood/kraken2/blob/master/docs/MANUAL.markdown
        // 1. Percentage of fragments covered by the clade rooted at this taxon
        // 2. Number of fragments covered by the clade rooted at this taxon
        // 3. Number of fragments assigned directly to this taxon
        // * 4. Number of minimizers in read data associated with this taxon (new)
        // * 5. An estimate of the number of distinct minimizers in read data
        //    associated with this taxon (new)
        // 6. A major rank abbreviation: (U)nclassified, (R)oot, (D)omain,
        //    (K)ingdom, (P)hylum, (C)lass, (O)rder, (F)amily, (G)enus or (S)pecies.
        //    Intermediate ranks append a depth indicating the distance from the
        //    nearest ancestor at a major rank. For example, "G2" denotes a taxon
        //    two levels below its genus ancestor.
        // 7. NCBI taxonomic ID number
        // 8. Indented scientific name
        let (
            percentage,
            clade_reads,
            direct_reads,
            minimizer_count,
            distinct_minimizer_count,
            rank,
            taxid,
            indented_taxon,
        ) = match fields.as_slice() {
            [percentage, clade_reads, direct_reads, rank, taxid, taxon] => (
                *percentage,
                *clade_reads,
                *direct_reads,
                None,
                None,
                *rank,
                *taxid,
                *taxon,
            ),
            [percentage, clade_reads, direct_reads, minimizer_count, distinct_minimizer_count, rank, taxid, taxon] => {
                (
                    *percentage,
                    *clade_reads,
                    *direct_reads,
                    Some(Self::parse_usize(minimizer_count, "minimizer count")?),
                    Some(Self::parse_usize(
                        distinct_minimizer_count,
                        "distinct minimizer count",
                    )?),
                    *rank,
                    *taxid,
                    *taxon,
                )
            }
            _ => {
                return Err(ParseErrorKind::InvalidFieldCount {
                    actual: fields.len(),
                });
            }
        };

        rank.first().ok_or(ParseErrorKind::MissingRank)?;
        let indentation = indented_taxon
            .iter()
            .take_while(|byte| **byte == b' ')
            .count();

        if indentation % 2 != 0 {
            return Err(ParseErrorKind::InvalidTaxonIndentation);
        }

        let taxon = &indented_taxon[indentation..];
        if taxid.is_empty() {
            return Err(ParseErrorKind::MissingTaxid);
        }
        if taxon.is_empty() {
            return Err(ParseErrorKind::MissingTaxonName);
        }

        let rank = Self::parse_text(rank, "rank")?;
        let taxid = Self::parse_text(taxid, "taxid")?;
        let taxon = Self::parse_text(taxon, "taxon")?;

        let percentage = Self::parse_float(percentage, "percentage")?;
        let clade_reads = Self::parse_usize(clade_reads, "clade reads")?;
        let direct_reads = Self::parse_usize(direct_reads, "direct reads")?;
        let taxon = Taxon::new(
            TaxonLevel::parse(rank)?,
            Taxid::new(taxid.to_owned())?,
            taxon.to_owned(),
        );
        let hierarchy_depth = indentation / 2;
        if taxon.is_unclassified() {
            return Ok(None);
        }

        // Only complete, classified rows advance the lineage state.
        let lineage = state.advance(&taxon, hierarchy_depth);

        Ok(Some(KrakenReportEntry::new(
            percentage,
            clade_reads,
            direct_reads,
            minimizer_count,
            distinct_minimizer_count,
            taxon,
            lineage,
            hierarchy_depth,
        )))
    }

    fn parse_text<'a>(value: &'a [u8], field: &'static str) -> Result<&'a str, ParseErrorKind> {
        str::from_utf8(value).map_err(|source| ParseErrorKind::InvalidUtf8 { field, source })
    }

    fn parse_float(value: &[u8], field: &'static str) -> Result<f64, ParseErrorKind> {
        let value = str::from_utf8(value.trim_ascii())
            .map_err(|source| ParseErrorKind::InvalidUtf8 { field, source })?;

        value
            .parse()
            .map_err(|source| ParseErrorKind::InvalidFloat {
                field,
                value: value.to_owned(),
                source,
            })
    }

    fn parse_usize(value: &[u8], field: &'static str) -> Result<usize, ParseErrorKind> {
        let value = str::from_utf8(value.trim_ascii())
            .map_err(|source| ParseErrorKind::InvalidUtf8 { field, source })?;

        value
            .parse()
            .map_err(|source| ParseErrorKind::InvalidInteger {
                field,
                value: value.to_owned(),
                source,
            })
    }
}

/// A report entry could not be parsed.
#[derive(thiserror::Error)]
#[error(transparent)]
pub struct ParseError(ParseErrorKind);

impl fmt::Debug for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

// Failures produced while decoding and assembling a Kraken2 report.
#[derive(Debug, thiserror::Error)]
enum ParseErrorKind {
    #[error("Invalid line with {actual} fields; expected 6 or 8")]
    InvalidFieldCount { actual: usize },

    #[error("Missing rank")]
    MissingRank,

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
        KrakenReportEntry, KrakenReportParser, LineageState, ParseError, ParseErrorKind, Taxid,
        Taxon, TaxonLevel,
    };

    fn parse(contents: &[u8]) -> Result<Vec<KrakenReportEntry>, ParseError> {
        let mut state = LineageState::new();
        contents
            .strip_suffix(b"\n")
            .unwrap_or(contents)
            .split(|byte| *byte == b'\n')
            .filter_map(|line| KrakenReportParser::parse_entry(line, &mut state).transpose())
            .collect()
    }

    #[test]
    fn skips_unclassified_rows_and_preserves_hierarchy_depths() {
        let entries = parse(
            b"20.00\t2\t2\tU\t0\tunclassified\n\
             100.00\t10\t0\tR\t1\troot\n\
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
                .map(KrakenReportEntry::hierarchy_depth)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(entries[2].taxon().term(), "Escherichia coli");
    }

    #[test]
    fn unclassified_rows_return_no_entry() {
        let mut state = LineageState::new();
        for minimizers in ["", "20\t5\t"] {
            for rank in ["U", "U1"] {
                let line = format!("20\t1\t1\t{minimizers}{rank}\t0\tunclassified");
                assert!(KrakenReportParser::parse_entry(line.as_bytes(), &mut state)
                    .unwrap()
                    .is_none());
            }
        }
    }

    #[test]
    fn malformed_unclassified_rows_still_return_errors() {
        for minimizers in ["", "20\t5\t"] {
            let invalid_percentage = format!("invalid\t1\t1\t{minimizers}U\t0\tunclassified");
            assert!(matches!(
                parse(invalid_percentage.as_bytes()),
                Err(ParseError(ParseErrorKind::InvalidFloat {
                    field: "percentage",
                    ..
                }))
            ));
            let invalid_taxid = format!("20\t1\t1\t{minimizers}U\t00\tunclassified");
            assert!(matches!(
                parse(invalid_taxid.as_bytes()),
                Err(ParseError(ParseErrorKind::InvalidTaxid(_)))
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
    fn blank_unclassified_and_invalid_lines_preserve_lineage() {
        for (line, fails) in [
            (b" \t\r\n".as_slice(), false),
            (b"20\t1\t1\tU\t0\tunclassified", false),
            (b"broken", true),
            (b"invalid\t4\t0\tD\t3\tArchaea", true),
            (b"100\t4\t0\tG2\t02\t  Genus", true),
        ] {
            let mut state = LineageState::new();
            KrakenReportParser::parse_entry(b"100\t4\t0\tD\t2\tBacteria", &mut state)
                .unwrap()
                .unwrap();
            assert_eq!(
                KrakenReportParser::parse_entry(line, &mut state).is_err(),
                fails
            );
            let species =
                KrakenReportParser::parse_entry(b"100\t4\t4\tS\t11\t  Species", &mut state)
                    .unwrap()
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
    fn parsing_skips_blank_lines_between_report_rows() {
        let entries = parse(
            b"\n \t\r\n100\t4\t0\tD\t2\tBacteria\n\t\n100\t4\t4\t20\t5\tS\t11\t  Species\n \n",
        )
        .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].taxon().taxid().as_str(), "2");
        assert_eq!(entries[1].taxon().taxid().as_str(), "11");
    }

    #[test]
    fn blank_lines_are_not_report_rows() {
        let mut state = LineageState::new();
        for line in [b"".as_slice(), b" ", b"\t", b" \t\r\n\x0c"] {
            assert!(matches!(
                KrakenReportParser::parse_entry(line, &mut state),
                Ok(None)
            ));
        }
    }

    #[test]
    fn nonblank_malformed_lines_are_errors() {
        let mut state = LineageState::new();
        for line in [b"broken".as_slice(), b" \tbroken\r", b"\0", b"\xc2\xa0"] {
            assert!(matches!(
                KrakenReportParser::parse_entry(line, &mut state),
                Err(ParseError(ParseErrorKind::InvalidFieldCount { .. }))
            ));
        }
    }

    #[test]
    fn rejects_missing_rank() {
        let error = parse(b"100.00\t10\t0\t\t1\troot\n").expect_err("expected an error");

        assert!(matches!(error.0, ParseErrorKind::MissingRank));
    }

    #[test]
    fn rejects_odd_taxon_indentation() {
        let error = parse(b"100.00\t10\t1\tD\t2\t Bacteria\n").expect_err("expected an error");

        assert!(matches!(error.0, ParseErrorKind::InvalidTaxonIndentation));
    }

    #[test]
    fn reports_the_invalid_numeric_field() {
        let error = parse(b"invalid\t10\t0\tR\t1\troot\n").expect_err("expected an error");

        assert!(matches!(
            error.0,
            ParseErrorKind::InvalidFloat {
                field: "percentage",
                ..
            }
        ));
    }

    #[test]
    fn rejects_invalid_utf8_in_domain_text() {
        let contents = b"100.00\t10\t1\tD\t2\t  \xFF\n";
        let error = parse(contents).expect_err("expected an error");

        assert!(matches!(
            error.0,
            ParseErrorKind::InvalidUtf8 { field: "taxon", .. }
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
        let mut state = LineageState::new();
        let bacteria = taxon("D", "2");
        let genus = taxon("G", "10");
        let species = taxon("S", "11");

        assert!(state.advance(&bacteria, 0).is_empty());
        assert_eq!(state.advance(&genus, 1), slice::from_ref(&bacteria));
        assert_eq!(state.advance(&species, 2), [bacteria, genus]);
    }

    #[test]
    fn siblings_share_ancestors() {
        let mut state = LineageState::new();
        let genus = taxon("G", "10");
        state.advance(&genus, 0);

        let first = state.advance(&taxon("S", "11"), 1);
        let second = state.advance(&taxon("S", "12"), 1);
        assert_eq!(first, slice::from_ref(&genus));
        assert_eq!(second, [genus]);
    }

    #[test]
    fn changing_branches_replaces_ancestors_below_the_shared_parent() {
        let mut state = LineageState::new();
        let bacteria = taxon("D", "2");
        let next_genus = taxon("G", "20");
        state.advance(&bacteria, 0);
        state.advance(&taxon("G", "10"), 1);
        state.advance(&taxon("S", "11"), 2);

        assert_eq!(state.advance(&next_genus, 1), slice::from_ref(&bacteria));
        assert_eq!(state.advance(&taxon("S", "21"), 2), [bacteria, next_genus]);
    }

    #[test]
    fn a_new_top_level_taxon_clears_the_previous_ancestry() {
        let mut state = LineageState::new();
        state.advance(&taxon("D", "2"), 0);
        state.advance(&taxon("S", "11"), 1);
        let archaea = taxon("D", "3");

        assert!(state.advance(&archaea, 0).is_empty());
        assert_eq!(state.advance(&taxon("S", "31"), 1), [archaea]);
    }

    #[test]
    fn a_missing_immediate_parent_starts_a_new_lineage() {
        let mut state = LineageState::new();
        state.advance(&taxon("D", "2"), 0);
        let genus = taxon("G", "10");

        assert!(state.advance(&genus, 2).is_empty());
        assert_eq!(state.advance(&taxon("S", "11"), 3), [genus]);
    }

    #[test]
    fn root_taxa_never_become_ancestors() {
        for root_level in ["R", "R1"] {
            let mut state = LineageState::new();
            let bacteria = taxon("D", "2");
            assert!(state.advance(&taxon(root_level, "1"), 0).is_empty());
            assert!(state.advance(&bacteria, 1).is_empty());
            assert_eq!(state.advance(&taxon("G", "10"), 2), [bacteria]);
        }
    }

    #[test]
    fn report_hierarchy_is_independent_of_intermediate_rank_depth() {
        let mut state = LineageState::new();
        let bacteria = taxon("D", "2");
        let genus = taxon("G2", "10");
        state.advance(&bacteria, 0);

        assert_eq!(state.advance(&genus, 1), slice::from_ref(&bacteria));
        assert_eq!(state.advance(&taxon("S", "11"), 2), [bacteria, genus]);
    }
}
