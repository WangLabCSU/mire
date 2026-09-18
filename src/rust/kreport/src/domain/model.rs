use super::error::TaxonomyError;
type Result<T> = std::result::Result<T, TaxonomyError>;

/// A taxonomic unit identified by its rank, taxid, and name.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Taxon {
    rank: String,
    taxid: String,
    name: String,
}

impl Taxon {
    pub(crate) fn new(rank: String, taxid: String, name: String) -> Self {
        Self { rank, taxid, name }
    }

    pub(crate) fn rank(&self) -> &str {
        &self.rank
    }

    pub(crate) fn taxid(&self) -> &str {
        &self.taxid
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

/// A key that uniquely identifies a taxon in a taxonomy.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum TaxonKey {
    Taxid(String),
    RankedName { rank: String, name: String },
}

impl TaxonKey {
    pub(crate) fn parse(mut value: String) -> Result<Self> {
        if value.is_empty() {
            return Err(TaxonomyError::Empty);
        }

        if value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(Self::taxid(value));
        }

        let separator = value
            .find("__")
            .ok_or_else(|| TaxonomyError::InvalidFormat(value.clone()))?;
        if separator == 0 || separator + 2 == value.len() {
            return Err(TaxonomyError::EmptyPart(value));
        }

        let name = value.split_off(separator + 2);
        value.truncate(separator);

        Ok(Self::ranked_name(value, name))
    }

    pub(crate) fn taxid(taxid: String) -> Self {
        Self::Taxid(taxid)
    }

    pub(crate) fn ranked_name(rank: String, name: String) -> Self {
        Self::RankedName { rank, name }
    }

    pub(crate) fn is_match(&self, taxon: &Taxon) -> bool {
        match self {
            Self::Taxid(taxid) => taxon.taxid() == taxid.as_str(),
            Self::RankedName { rank, name } => {
                taxon.rank() == rank.as_str() && taxon.name() == name.as_str()
            }
        }
    }

    pub(crate) fn any_match(&self, lineage: &[Taxon]) -> bool {
        lineage.iter().any(|taxon| self.is_match(taxon))
    }
}

#[cfg(test)]
mod tests {
    use super::{Taxon, TaxonKey};

    #[test]
    fn taxon_keys_match_by_taxid_or_ranked_name() {
        let taxon = Taxon::new("D".to_owned(), "2".to_owned(), "Bacteria".to_owned());

        assert!(TaxonKey::taxid("2".to_owned()).is_match(&taxon));
        assert!(TaxonKey::ranked_name("D".to_owned(), "Bacteria".to_owned()).is_match(&taxon));
        assert!(!TaxonKey::ranked_name("S".to_owned(), "Bacteria".to_owned()).is_match(&taxon));

        let lineage = vec![taxon];
        assert!(TaxonKey::taxid("2".to_owned()).any_match(&lineage));
    }

    #[test]
    fn parses_taxon_keys() {
        assert_eq!(
            TaxonKey::parse("562".to_owned()).unwrap(),
            TaxonKey::taxid("562".to_owned())
        );
        assert_eq!(
            TaxonKey::parse("D__Bacteria".to_owned()).unwrap(),
            TaxonKey::ranked_name("D".to_owned(), "Bacteria".to_owned())
        );
    }

    #[test]
    fn rejects_empty_taxon_key() {
        let error = TaxonKey::parse(String::new()).expect_err("expected empty taxonomy to fail");

        assert_eq!(error.to_string(), "Taxonomy must not be empty.");
    }
}

/// One row from a Kraken2 report.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KrakenReportEntry {
    percentage: f64,
    clade_reads: usize,
    direct_reads: usize,
    minimizer_count: Option<usize>,
    distinct_minimizer_count: Option<usize>,
    taxon: Taxon,
    lineage: Vec<Taxon>,
    level: usize,
}

pub(crate) struct KrakenReportEntryParts {
    pub(crate) percentage: f64,
    pub(crate) clade_reads: usize,
    pub(crate) direct_reads: usize,
    pub(crate) minimizer_count: Option<usize>,
    pub(crate) distinct_minimizer_count: Option<usize>,
    pub(crate) taxon: Taxon,
    pub(crate) lineage: Vec<Taxon>,
}

impl KrakenReportEntry {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        percentage: f64,
        clade_reads: usize,
        direct_reads: usize,
        minimizer_count: Option<usize>,
        distinct_minimizer_count: Option<usize>,
        taxon: Taxon,
        lineage: Vec<Taxon>,
        level: usize,
    ) -> Self {
        Self {
            percentage,
            clade_reads,
            direct_reads,
            minimizer_count,
            distinct_minimizer_count,
            taxon,
            lineage,
            level,
        }
    }

    #[cfg(test)]
    pub(crate) fn minimizer_count(&self) -> Option<usize> {
        self.minimizer_count
    }

    #[cfg(test)]
    pub(crate) fn distinct_minimizer_count(&self) -> Option<usize> {
        self.distinct_minimizer_count
    }

    pub(crate) fn taxon(&self) -> &Taxon {
        &self.taxon
    }

    pub(crate) fn lineage(&self) -> &[Taxon] {
        &self.lineage
    }

    pub(crate) fn level(&self) -> usize {
        self.level
    }

    pub(crate) fn is_lineage_ancestor(&self) -> bool {
        !matches!(self.taxon.rank().as_bytes().first(), Some(b'R' | b'U'))
    }

    pub(crate) fn into_parts(self) -> KrakenReportEntryParts {
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

/// A parsed Kraken2 report.
#[derive(Debug, PartialEq)]
pub(crate) struct KrakenReport {
    entries: Vec<KrakenReportEntry>,
}

impl KrakenReport {
    pub(crate) fn new(entries: Vec<KrakenReportEntry>) -> Self {
        Self { entries }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[KrakenReportEntry] {
        &self.entries
    }

    pub(crate) fn into_entries(self) -> Vec<KrakenReportEntry> {
        self.entries
    }

    pub(crate) fn retain(self, predicate: impl FnMut(&KrakenReportEntry) -> bool) -> KrakenReport {
        Self::new(self.entries.into_iter().filter(predicate).collect())
    }
}
