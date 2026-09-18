use std::collections::HashSet;

use super::error::TaxonomyError;
type Result<T> = std::result::Result<T, TaxonomyError>;

use super::model::{KrakenReport, TaxonKey};

/// A validated set of taxonomic groups used to select report entries.
#[derive(Debug)]
pub(crate) struct TaxonomySelection {
    keys: HashSet<TaxonKey>,
    labels: Vec<String>,
}

impl TaxonomySelection {
    pub(crate) fn parse(labels: &[&str]) -> Result<Self> {
        let keys = labels
            .iter()
            .map(|label| TaxonKey::parse((*label).to_owned()))
            .collect::<Result<HashSet<_>>>()?;

        if keys.is_empty() {
            return Err(TaxonomyError::EmptySelection);
        }

        Ok(Self {
            keys,
            labels: labels.iter().map(|label| (*label).to_owned()).collect(),
        })
    }

    pub(crate) fn apply(self, report: KrakenReport) -> Result<KrakenReport> {
        let report =
            report.retain(|entry| self.keys.iter().any(|key| key.any_match(entry.lineage())));

        if report.is_empty() {
            return Err(TaxonomyError::NoMatches(self.labels));
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::TaxonKey;

    #[test]
    fn rejects_malformed_taxonomy() {
        let error = TaxonKey::parse("Bacteria".to_owned())
            .expect_err("expected malformed taxonomy to fail");

        assert!(error
            .to_string()
            .contains("Expected a taxid or 'rank__name'"));
    }

    #[test]
    fn parses_taxid_key() {
        assert!(TaxonKey::parse("562".to_owned()).is_ok());
    }

    #[test]
    fn rejects_empty_taxonomy_parts() {
        for taxonomy in ["__Bacteria", "D__"] {
            assert!(TaxonKey::parse(taxonomy.to_owned()).is_err());
        }
    }
}
