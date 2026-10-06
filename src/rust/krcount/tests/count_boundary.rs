//! Read and k-mer counting through the public file API.
use std::fs;

use mire_krcount::{count_reads, CountRequest};
use tempfile::tempdir;

#[test]
fn count_tables_preserve_report_order_and_counts_with_or_without_umi() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("report");
    let input = dir.path().join("reads");
    fs::write(
        &report,
        "100\t2\t0\tD\t2\tBacteria\n100\t2\t2\tS\t11\t  Species\n",
    )
    .unwrap();
    fs::write(
        &input,
        "11\tBARCODE:bc UMI:u\t11:2\tACGT\tIIII\n11\tBARCODE:bc UMI:u\t11:2\tACGT\tIIII\n",
    )
    .unwrap();
    for umi_tag in [None, Some("UMI".into())] {
        let table = count_reads(
            CountRequest {
                report: report.to_str().unwrap(),
                taxonomy: None,
                input: input.to_str().unwrap(),
                umi_tag,
                barcode_tag: Some("BARCODE".into()),
            },
            1,
            Some(0),
        )
        .unwrap();
        assert_eq!(table.ranks, ["D", "S"]);
        assert_eq!(table.taxa[1], [None, Some("Species".into())]);
        assert_eq!(table.barcodes, ["bc"]);
        // Existing policy counts reads even when their UMI values are equal.
        assert_eq!(table.counts, [vec![Some(2), Some(2)]]);
        assert_eq!(table.kmer_total, [vec![Some(4), Some(4)]]);
        assert_eq!(table.kmer_unique, [vec![Some(2), Some(2)]]);
    }
}

#[test]
fn report_errors_remain_structured_at_count_boundary() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("malformed");
    fs::write(&report, "broken\n").unwrap();
    let result = count_reads(
        CountRequest {
            report: report.to_str().unwrap(),
            taxonomy: None,
            input: "unused",
            umi_tag: None,
            barcode_tag: None,
        },
        1,
        Some(1),
    );
    let error = result.err().unwrap();
    assert!(error.to_string().contains("line 1"));
    assert!(error.to_string().contains(report.to_str().unwrap()));
    let report_error = std::error::Error::source(&error).unwrap();
    assert_eq!(
        report_error.source().unwrap().to_string(),
        "Invalid line with 1 fields; expected 6 or 8"
    );
}

#[test]
fn reports_without_selected_entries_produce_empty_count_tables() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("report");
    let input = dir.path().join("reads");
    fs::write(&input, "11\tBARCODE:bc\t11:2\tACGT\tIIII\n").unwrap();
    for (contents, taxonomy) in [
        ("", None),
        ("100\t1\t1\tS\t11\tSpecies\n", Some(vec!["999".into()])),
    ] {
        fs::write(&report, contents).unwrap();
        let table = count_reads(
            CountRequest {
                report: report.to_str().unwrap(),
                taxonomy,
                input: input.to_str().unwrap(),
                umi_tag: None,
                barcode_tag: Some("BARCODE".into()),
            },
            1,
            Some(0),
        )
        .unwrap();
        assert!(table.ranks.is_empty());
        assert!(table.taxa.is_empty());
        assert!(table.barcodes.is_empty());
        assert!(table.counts.is_empty());
        assert!(table.kmer_total.is_empty());
        assert!(table.kmer_unique.is_empty());
    }
}

#[test]
fn filtered_reports_keep_ancestor_columns_and_skip_absent_taxids() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("report");
    let input = dir.path().join("reads");
    fs::write(&report, "100\t4\t0\tR\t1\troot\n100\t4\t0\tD\t2\t  Bacteria\n100\t4\t0\tG\t10\t    Genus\n100\t4\t4\tS\t11\t      Species\n").unwrap();
    fs::write(
        &input,
        "11\tBARCODE:bc\t11:2\tACGT\tIIII\n2\tBARCODE:excluded\t2:2\tACGT\tIIII\n",
    )
    .unwrap();
    let table = count_reads(
        CountRequest {
            report: report.to_str().unwrap(),
            taxonomy: Some(vec!["G__Genus".into()]),
            input: input.to_str().unwrap(),
            umi_tag: None,
            barcode_tag: Some("BARCODE".into()),
        },
        1,
        Some(0),
    )
    .unwrap();
    assert_eq!(table.ranks, ["D", "G", "S"]);
    assert_eq!(
        table.taxa,
        [
            vec![Some("Bacteria".into()); 2],
            vec![Some("Genus".into()); 2],
            vec![None, Some("Species".into())]
        ]
    );
    assert_eq!(table.barcodes, ["bc"]);
    assert_eq!(table.counts, [vec![Some(1), Some(1)]]);
    assert_eq!(table.kmer_total, [vec![Some(2), Some(2)]]);
    assert_eq!(table.kmer_unique, [vec![Some(2), Some(2)]]);
}
