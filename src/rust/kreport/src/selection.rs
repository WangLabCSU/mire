use bytes::Bytes;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

use super::domain::model::KrakenReportEntry;

/// A reusable lineage index shared by extraction and counting.
pub(crate) struct TaxonomyIndex {
    ancestors: HashMap<Bytes, HashSet<Bytes>>,
}

impl TaxonomyIndex {
    pub(crate) fn new(entries: &[KrakenReportEntry]) -> Self {
        let ancestors = entries
            .iter()
            .map(|entry| {
                (
                    Bytes::copy_from_slice(entry.taxon().taxid().as_bytes()),
                    entry
                        .lineage()
                        .iter()
                        .map(|taxon| Bytes::copy_from_slice(taxon.taxid().as_bytes()))
                        .collect(),
                )
            })
            .collect();
        Self { ancestors }
    }

    pub(crate) fn ancestors(&self, taxid: &[u8]) -> Option<&HashSet<Bytes>> {
        self.ancestors.get(taxid)
    }

    pub(crate) fn descendants(&self, selected: &HashSet<Bytes>) -> HashSet<Bytes> {
        self.ancestors
            .iter()
            .filter(|(_, ancestors)| !ancestors.is_disjoint(selected))
            .map(|(taxid, _)| taxid.clone())
            .collect()
    }
}

#[derive(Default)]
pub struct TaxonSelection {
    pub ranks: Option<HashSet<String>>,
    pub names: Option<HashSet<String>>,
    pub taxids: Option<HashSet<String>>,
    pub descendants: bool,
}

impl TaxonSelection {
    pub(crate) fn select(&self, entries: &[KrakenReportEntry]) -> HashSet<Bytes> {
        let selected = entries
            .iter()
            .filter(|entry| {
                let taxon = entry.taxon();
                self.ranks
                    .as_ref()
                    .is_none_or(|ranks| ranks.contains(taxon.rank()))
                    && self
                        .names
                        .as_ref()
                        .is_none_or(|names| names.contains(taxon.name()))
                    && self
                        .taxids
                        .as_ref()
                        .is_none_or(|taxids| taxids.contains(taxon.taxid()))
            })
            .map(|entry| Bytes::copy_from_slice(entry.taxon().taxid().as_bytes()))
            .collect();
        if self.descendants {
            TaxonomyIndex::new(entries).descendants(&selected)
        } else {
            selected
        }
    }
}

pub(crate) fn rank_order_key(rank: &str) -> (usize, usize) {
    let Some((&first, suffix)) = rank.as_bytes().split_first() else {
        return (10, 0);
    };
    let base = b"URDKPCOFGS"
        .iter()
        .position(|byte| *byte == first)
        .unwrap_or(10);
    let depth = if suffix.is_empty() {
        0
    } else {
        std::str::from_utf8(suffix)
            .ok()
            .and_then(|suffix| suffix.parse().ok())
            .unwrap_or(usize::MAX)
    };
    (base, depth)
}
