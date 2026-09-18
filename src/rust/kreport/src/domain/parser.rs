use std::str;

use super::{
    error::KrakenReportError,
    model::{KrakenReportEntry, Taxon},
};

type Result<T> = std::result::Result<T, KrakenReportError>;

/// Stateless parser for individual Kraken2 report rows.
pub(crate) struct KrakenReportParser;

impl KrakenReportParser {
    pub(crate) fn parse_line(line: &[u8]) -> Result<Option<ParsedReportRow>> {
        if line.iter().all(|byte| byte.is_ascii_whitespace()) {
            return Ok(None);
        }

        let fields = line.split(|byte| *byte == b'\t').collect::<Vec<_>>();

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
                return Err(KrakenReportError::InvalidFieldCount {
                    actual: fields.len(),
                });
            }
        };

        rank.first().ok_or(KrakenReportError::MissingRank)?;
        let indentation = indented_taxon
            .iter()
            .take_while(|byte| **byte == b' ')
            .count();

        if indentation % 2 != 0 {
            return Err(KrakenReportError::InvalidTaxonIndentation);
        }

        let taxon = &indented_taxon[indentation..];
        if taxid.is_empty() {
            return Err(KrakenReportError::MissingTaxid);
        }
        if taxon.is_empty() {
            return Err(KrakenReportError::MissingTaxonName);
        }

        let rank = Self::parse_text(rank, "rank")?;
        let taxid = Self::parse_text(taxid, "taxid")?;
        let taxon = Self::parse_text(taxon, "taxon")?;

        Ok(Some(ParsedReportRow {
            percentage: Self::parse_float(percentage, "percentage")?,
            clade_reads: Self::parse_usize(clade_reads, "clade reads")?,
            direct_reads: Self::parse_usize(direct_reads, "direct reads")?,
            minimizer_count,
            distinct_minimizer_count,
            taxon: Taxon::new(rank.to_owned(), taxid.to_owned(), taxon.to_owned()),
            level: indentation / 2,
        }))
    }

    fn parse_text<'a>(value: &'a [u8], field: &'static str) -> Result<&'a str> {
        str::from_utf8(value).map_err(|source| KrakenReportError::InvalidUtf8 { field, source })
    }

    fn parse_float(value: &[u8], field: &'static str) -> Result<f64> {
        let value = str::from_utf8(value.trim_ascii())
            .map_err(|source| KrakenReportError::InvalidUtf8 { field, source })?;

        value
            .parse()
            .map_err(|source| KrakenReportError::InvalidFloat {
                field,
                value: value.to_owned(),
                source,
            })
    }

    fn parse_usize(value: &[u8], field: &'static str) -> Result<usize> {
        let value = str::from_utf8(value.trim_ascii())
            .map_err(|source| KrakenReportError::InvalidUtf8 { field, source })?;

        value
            .parse()
            .map_err(|source| KrakenReportError::InvalidInteger {
                field,
                value: value.to_owned(),
                source,
            })
    }
}

pub(crate) struct ParsedReportRow {
    percentage: f64,
    clade_reads: usize,
    direct_reads: usize,
    minimizer_count: Option<usize>,
    distinct_minimizer_count: Option<usize>,
    taxon: Taxon,
    level: usize,
}

impl ParsedReportRow {
    pub(crate) fn level(&self) -> usize {
        self.level
    }

    pub(crate) fn taxon(&self) -> &Taxon {
        &self.taxon
    }

    pub(crate) fn into_entry(self, lineage: Vec<Taxon>) -> KrakenReportEntry {
        KrakenReportEntry::new(
            self.percentage,
            self.clade_reads,
            self.direct_reads,
            self.minimizer_count,
            self.distinct_minimizer_count,
            self.taxon,
            lineage,
            self.level,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{builder::KrakenReportBuilder, error::KrakenReportError, model::KrakenReport},
        KrakenReportParser,
    };

    fn parse(contents: &[u8]) -> Result<KrakenReport, KrakenReportError> {
        let mut builder = KrakenReportBuilder::new();
        for line in contents.split(|byte| *byte == b'\n') {
            if let Some(row) = KrakenReportParser::parse_line(line)? {
                builder.push(row)?;
            }
        }
        Ok(builder.finish())
    }

    #[test]
    fn parses_unclassified_rows_and_classified_lineages() {
        let report = parse(
            b"20.00\t2\t2\tU\t0\tunclassified\n\
             100.00\t10\t0\tR\t1\troot\n\
             100.00\t10\t1\tD\t2\t  Bacteria\n\
             50.00\t5\t5\tS\t562\t    Escherichia coli\n",
        )
        .unwrap();
        let entries = report.entries();

        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].taxon().rank(), "U");
        assert_eq!(entries[0].taxon().taxid(), "0");
        assert_eq!(entries[0].taxon().name(), "unclassified");
        assert_eq!(entries[0].lineage()[0].rank(), "U");
        assert_eq!(
            entries[3]
                .lineage()
                .iter()
                .map(|taxon| taxon.rank())
                .collect::<Vec<_>>(),
            ["D", "S"]
        );
        assert_eq!(
            entries[3]
                .lineage()
                .iter()
                .map(|taxon| taxon.name())
                .collect::<Vec<_>>(),
            ["Bacteria", "Escherichia coli"]
        );
    }

    #[test]
    fn preserves_optional_values_in_mixed_report_formats() {
        let report = parse(
            b"100.00\t10\t0\tR\t1\troot\n\
             100.00\t10\t1\t400\t40\tD\t2\t  Bacteria\n",
        )
        .unwrap();
        let entries = report.entries();

        assert_eq!(entries[0].minimizer_count(), None);
        assert_eq!(entries[0].distinct_minimizer_count(), None);
        assert_eq!(entries[1].minimizer_count(), Some(400));
        assert_eq!(entries[1].distinct_minimizer_count(), Some(40));
    }

    #[test]
    fn rejects_missing_rank_codes() {
        let error = parse(b"100.00\t10\t0\t\t1\troot\n").expect_err("expected an error");

        assert!(matches!(error, KrakenReportError::MissingRank));
    }

    #[test]
    fn rejects_odd_taxon_indentation() {
        let error = parse(b"100.00\t10\t1\tD\t2\t Bacteria\n").expect_err("expected an error");

        assert!(matches!(error, KrakenReportError::InvalidTaxonIndentation));
    }

    #[test]
    fn reports_the_invalid_numeric_field() {
        let error = parse(b"invalid\t10\t0\tR\t1\troot\n").expect_err("expected an error");

        assert!(matches!(
            error,
            KrakenReportError::InvalidFloat {
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
            error,
            KrakenReportError::InvalidUtf8 { field: "taxon", .. }
        ));
    }
}
