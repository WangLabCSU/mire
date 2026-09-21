use std::io::Read;

use bytes::Bytes;
use kreport::{Error, KrakenReportReader};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

pub(crate) struct ReportTaxonomy {
    pub(crate) taxids: Vec<Bytes>,
    pub(crate) ancestors: HashMap<Bytes, HashSet<Bytes>>,
    pub(crate) ranks: Vec<String>,
    pub(crate) taxa: Vec<Vec<Option<String>>>,
}

// Translate report entries into counting ancestry and ordered taxon columns.
pub(crate) fn read_taxonomy<R: Read>(
    mut reader: KrakenReportReader<R>,
) -> Result<ReportTaxonomy, Error> {
    reader
        .entries()
        .collect::<Result<Vec<_>, _>>()
        .map(|entries| {
            let taxids = entries
                .iter()
                .map(|entry| Bytes::copy_from_slice(entry.taxon().taxid().as_str().as_bytes()))
                .collect();
            // Duplicate taxids keep the last entry's ancestry.
            let ancestors = entries
                .iter()
                .map(|entry| {
                    (
                        Bytes::copy_from_slice(entry.taxon().taxid().as_str().as_bytes()),
                        entry
                            .lineage()
                            .iter()
                            .chain(std::iter::once(entry.taxon()))
                            .map(|taxon| Bytes::copy_from_slice(taxon.taxid().as_str().as_bytes()))
                            .collect(),
                    )
                })
                .collect();
            let mut ranks: Vec<_> = entries
                .iter()
                .flat_map(|entry| {
                    entry
                        .lineage()
                        .iter()
                        .chain(std::iter::once(entry.taxon()))
                        .map(|taxon| taxon.level())
                })
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            ranks.sort_unstable();
            let taxa = ranks
                .iter()
                .map(|rank| {
                    entries
                        .iter()
                        .map(|entry| {
                            entry
                                .lineage()
                                .iter()
                                .chain(std::iter::once(entry.taxon()))
                                .find(|taxon| taxon.level() == *rank)
                                .map(|taxon| taxon.term().to_owned())
                        })
                        .collect()
                })
                .collect();
            ReportTaxonomy {
                taxids,
                ancestors,
                ranks: ranks.into_iter().map(ToString::to_string).collect(),
                taxa,
            }
        })
}

#[cfg(test)]
mod tests {
    use kreport::KrakenReportReader;

    use super::read_taxonomy;

    #[test]
    fn lineage_columns_keep_rows_and_sort_intermediate_ranks_numerically() {
        let input = b"100\t4\t0\tD\t2\tBacteria\n100\t4\t0\tG2\t10\t  Genus group\n100\t4\t0\tG10\t11\t    Subgroup\n100\t4\t4\tS\t12\t      Species\n";
        let report = read_taxonomy(KrakenReportReader::new(input.as_slice())).unwrap();
        let ranks = report.ranks;
        let columns = report.taxa;
        assert_eq!(ranks, ["D", "G2", "G10", "S"]);
        assert_eq!(columns[0], vec![Some("Bacteria".into()); 4]);
        assert_eq!(
            columns[1],
            [
                None,
                Some("Genus group".into()),
                Some("Genus group".into()),
                Some("Genus group".into())
            ]
        );
        assert_eq!(
            columns[2],
            [None, None, Some("Subgroup".into()), Some("Subgroup".into())]
        );
        assert_eq!(columns[3], [None, None, None, Some("Species".into())]);
    }
}
