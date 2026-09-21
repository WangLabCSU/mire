use bytes::Bytes;
use mire_sequence::ReadFragment;
use mire_streaming::{RecordExecutor, RecordSink, RecordSource, Result};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

use crate::domain::{
    joined::{ClassifiedRead, ReadJoin},
    output::{Classification, ClassificationFilter},
};

use crate::report::ReportTaxon;

pub(crate) type Classifications = HashMap<Bytes, Classification>;

/// Intersect supplied filters before optionally including descendants.
/// An absent filter is unrestricted; an empty filter matches nothing.
pub(crate) fn select_taxids(
    report: &[ReportTaxon],
    ranks: Option<&HashSet<String>>,
    names: Option<&HashSet<String>>,
    taxids: Option<&HashSet<String>>,
    descendants: bool,
) -> HashSet<Bytes> {
    // Normalize a single-zero spelling (G0 -> G); other labels must match
    // the report's taxonomic level exactly.
    let ranks = ranks.map(|ranks| {
        ranks
            .iter()
            .map(|rank| {
                rank.strip_suffix('0')
                    .filter(|prefix| prefix.len() == 1)
                    .unwrap_or(rank)
            })
            .collect::<HashSet<_>>()
    });
    let selected: HashSet<Bytes> = report
        .iter()
        .filter(|entry| {
            ranks
                .as_ref()
                .is_none_or(|ranks| ranks.contains(entry.rank.as_str()))
                && names.is_none_or(|names| names.contains(&entry.term))
                && taxids.is_none_or(|taxids| taxids.contains(&entry.taxid))
        })
        .map(|entry| Bytes::copy_from_slice(entry.taxid.as_bytes()))
        .collect();
    if !descendants {
        return selected;
    }

    // Preserve the previous index's last-entry ancestry for duplicate taxids.
    report
        .iter()
        .map(|entry| (&entry.taxid, entry))
        .collect::<HashMap<_, _>>()
        .into_iter()
        .filter(|(_, entry)| {
            entry
                .lineage
                .iter()
                .chain(std::iter::once(&entry.taxid))
                .any(|taxid| selected.contains(taxid.as_bytes()))
        })
        .map(|(taxid, _)| Bytes::copy_from_slice(taxid.as_bytes()))
        .collect()
}

/// Filter original Kraken lines without changing their serialized representation.
pub(crate) fn extract_classifications<S, W, X>(
    source: S,
    sink: &mut W,
    executor: &X,
    filter: &ClassificationFilter,
) -> Result<()>
where
    S: RecordSource<Record = Bytes> + Send,
    W: RecordSink<Bytes>,
    X: RecordExecutor,
{
    executor.execute(source, sink, |line| {
        Ok::<_, std::convert::Infallible>(filter.matches_line(&line).then_some(line))
    })
}

struct ClassificationCollector(Classifications);

impl RecordSink<Classification> for ClassificationCollector {
    fn write_record(&mut self, record: Classification) -> Result<()> {
        self.0.insert(record.read_id.clone(), record);
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

pub(crate) fn load_classifications<S, X>(
    source: S,
    executor: &X,
    filter: &ClassificationFilter,
) -> Result<Classifications>
where
    S: RecordSource<Record = Bytes> + Send,
    X: RecordExecutor,
{
    let mut sink = ClassificationCollector(HashMap::default());
    executor.execute(source, &mut sink, |line| {
        Ok::<_, crate::domain::output::KrakenOutputError>(
            if filter.accepts_classified_line(&line) {
                Classification::parse(&line)?
            } else {
                None
            },
        )
    })?;
    Ok(sink.0)
}

pub(crate) fn read_ids(source: &mut impl RecordSource<Record = Bytes>) -> Result<HashSet<Bytes>> {
    let mut ids = HashSet::default();
    while let Some(line) = source.next_record()? {
        // ID extraction only uses column two; it does not validate classifications.
        if let Ok(line) = std::str::from_utf8(&line) {
            if let Some(id) = line.split('\t').nth(1).filter(|id| !id.is_empty()) {
                ids.insert(Bytes::copy_from_slice(id.as_bytes()));
            }
        }
    }
    Ok(ids)
}

pub(crate) fn join_reads<S, W, X>(
    source: S,
    sink: &mut W,
    executor: &X,
    classifications: &Classifications,
    join: &ReadJoin,
) -> Result<()>
where
    S: RecordSource<Record = ReadFragment> + Send,
    W: RecordSink<ClassifiedRead>,
    X: RecordExecutor,
{
    executor.execute(source, sink, |fragment| {
        classifications
            .get(fragment.id())
            .map(|classification| join.join(classification, fragment))
            .transpose()
    })
}

#[cfg(test)]
mod tests {
    use kreport::KrakenReportReader;

    use super::select_taxids;
    use crate::report::{read_taxa, ReportTaxon};

    fn report() -> Vec<ReportTaxon> {
        let input = b"100\t4\t0\tR\t1\troot\n100\t4\t0\tD\t2\t  Bacteria\n100\t4\t0\tG\t10\t    Genus\n50\t2\t2\tS\t11\t      Species A\n50\t2\t2\tS\t12\t      Species B\n";
        parse_report(input)
    }

    fn parse_report(input: &[u8]) -> Vec<ReportTaxon> {
        read_taxa(KrakenReportReader::new(input)).unwrap()
    }

    #[test]
    fn root_and_leaf_selection_preserve_ancestor_membership() {
        let report = report();
        for taxid in ["1", "11"] {
            let taxids = [taxid.to_owned()].into_iter().collect();
            let selected = select_taxids(&report, None, None, Some(&taxids), true);
            assert_eq!(selected.len(), 1);
            assert!(selected.contains(taxid.as_bytes()));
        }
    }

    #[test]
    fn rank_filters_match_valid_ranks_exactly() {
        let report = parse_report(b"100\t4\t4\tG2\t10\tGenus group\n");
        for (rank, matches) in [
            ("G002", false),
            ("G02", false),
            ("G0", false),
            ("G2", true),
            ("G", false),
            ("G256", false),
            ("X", false),
        ] {
            let ranks = [rank.to_owned()].into_iter().collect();
            let selected = select_taxids(&report, Some(&ranks), None, None, false);
            assert_eq!(selected.contains(b"10".as_slice()), matches, "{rank}");
        }
    }

    #[test]
    fn rank_filters_treat_zero_as_no_intermediate_level() {
        for rank in ["R", "D", "K", "P", "C", "O", "F", "G", "S"] {
            let input = format!("100\t4\t4\t{rank}\t10\tTaxon\n");
            let report = parse_report(input.as_bytes());
            for label in [rank.to_owned(), format!("{rank}0")] {
                let ranks = [label.clone()].into_iter().collect();
                let selected = select_taxids(&report, Some(&ranks), None, None, false);
                assert_eq!(selected.len(), 1, "{label}");
                assert!(selected.contains(b"10".as_slice()), "{label}");
            }
        }
    }

    #[test]
    fn rank_filters_reject_noncanonical_rank_notation() {
        let report = report();
        for label in [
            "", "G00", "G01", "G02", "G000", "G+0", "G+1", "G-0", "G-1", " G", "G ", "g", "Genus",
            "G__Genus", "Ｇ", "G２", "G256",
        ] {
            let ranks = [label.to_owned()].into_iter().collect();
            assert!(
                select_taxids(&report, Some(&ranks), None, None, false).is_empty(),
                "{label:?}"
            );
        }
    }

    #[test]
    fn rank_filters_match_maximum_intermediate_depth() {
        let report = parse_report(b"100\t4\t4\tG255\t10\tGenus group\n");
        for (label, matches) in [
            ("G255", true),
            ("G", false),
            ("G0", false),
            ("G0255", false),
            ("G256", false),
        ] {
            let ranks = [label.to_owned()].into_iter().collect();
            let selected = select_taxids(&report, Some(&ranks), None, None, false);
            assert_eq!(selected.contains(b"10".as_slice()), matches, "{label}");
        }
    }

    #[test]
    fn duplicate_taxids_use_the_last_entry_ancestry() {
        let report = parse_report(b"100\t4\t0\tD\t2\tBacteria\n100\t4\t0\tG\t10\t  First genus\n100\t4\t4\tS\t11\t    Species\n100\t4\t0\tG\t20\t  Last genus\n100\t4\t4\tS\t11\t    Species\n");
        for (taxid, expected) in [("10", vec!["10"]), ("20", vec!["20", "11"])] {
            let taxids = [taxid.to_owned()].into_iter().collect();
            let selected = select_taxids(&report, None, None, Some(&taxids), true);
            assert_eq!(selected.len(), expected.len());
            for taxid in expected {
                assert!(selected.contains(taxid.as_bytes()));
            }
        }
    }

    #[test]
    fn taxid_filters_intersect_rank_name_and_taxid() {
        let report = report();
        let ranks = ["S".to_owned()].into_iter().collect();
        let names = ["Species A".to_owned()].into_iter().collect();
        let taxids = ["11".to_owned(), "12".to_owned()].into_iter().collect();
        let selected = select_taxids(&report, Some(&ranks), Some(&names), Some(&taxids), false);
        assert_eq!(selected.len(), 1);
        assert!(selected.contains(b"11".as_slice()));
        let taxids = ["12".to_owned()].into_iter().collect();
        assert!(
            select_taxids(&report, Some(&ranks), Some(&names), Some(&taxids), false).is_empty()
        );
        let ranks = ["G".to_owned()].into_iter().collect();
        assert!(select_taxids(&report, Some(&ranks), Some(&names), None, false).is_empty());
    }

    #[test]
    fn absent_taxid_filters_and_empty_filters_have_different_meanings() {
        let report = report();
        let empty = Default::default();
        for descendants in [false, true] {
            assert_eq!(
                select_taxids(&report, None, None, None, descendants).len(),
                5
            );
            assert!(select_taxids(&report, Some(&empty), None, None, descendants).is_empty());
            assert!(select_taxids(&report, None, Some(&empty), None, descendants).is_empty());
            assert!(select_taxids(&report, None, None, Some(&empty), descendants).is_empty());
        }
    }

    #[test]
    fn descendants_are_included_after_matching_all_taxid_filters() {
        let report = report();
        let ranks = ["G".to_owned()].into_iter().collect();
        let names = ["Genus".to_owned()].into_iter().collect();
        let taxids = ["10".to_owned()].into_iter().collect();
        let selected = select_taxids(&report, Some(&ranks), Some(&names), Some(&taxids), false);
        assert_eq!(selected.len(), 1);
        assert!(selected.contains(b"10".as_slice()));
        let selected = select_taxids(&report, Some(&ranks), Some(&names), Some(&taxids), true);
        assert_eq!(selected.len(), 3);
        for taxid in [b"10", b"11", b"12"] {
            assert!(selected.contains(taxid.as_slice()));
        }
    }
}
