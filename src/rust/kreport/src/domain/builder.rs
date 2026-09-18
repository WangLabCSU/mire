use super::{
    error::KrakenReportError,
    model::{KrakenReport, KrakenReportEntry},
    parser::ParsedReportRow,
};

type Result<T> = std::result::Result<T, KrakenReportError>;

/// Assembles parsed rows into a report and reconstructs taxonomic lineages.
pub(crate) struct KrakenReportBuilder {
    entries: Vec<KrakenReportEntry>,
    ancestor_indices: Vec<usize>,
}

impl KrakenReportBuilder {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::with_capacity(10),
            ancestor_indices: Vec::with_capacity(10),
        }
    }

    pub(crate) fn push(&mut self, row: ParsedReportRow) -> Result<()> {
        self.discard_non_parent_ancestors(row.level())?;

        let ancestors = self
            .ancestor_indices
            .iter()
            .map(|index| {
                self.entries
                    .get(*index)
                    .ok_or(KrakenReportError::InvalidAncestryState)
            })
            .collect::<Result<Vec<_>>>()?;

        let lineage = ancestors
            .iter()
            .map(|ancestor| ancestor.taxon().clone())
            .chain(std::iter::once(row.taxon().clone()))
            .collect();
        let entry = row.into_entry(lineage);
        let entry_index = self.entries.len();

        if entry.is_lineage_ancestor() {
            self.ancestor_indices.push(entry_index);
        }
        self.entries.push(entry);

        Ok(())
    }

    pub(crate) fn finish(self) -> KrakenReport {
        KrakenReport::new(self.entries)
    }

    fn discard_non_parent_ancestors(&mut self, level: usize) -> Result<()> {
        while let Some(index) = self.ancestor_indices.last() {
            let ancestor_level = self
                .entries
                .get(*index)
                .map(KrakenReportEntry::level)
                .ok_or(KrakenReportError::InvalidAncestryState)?;

            if level > 0 && ancestor_level == level - 1 {
                break;
            }
            self.ancestor_indices.pop();
        }

        Ok(())
    }
}
