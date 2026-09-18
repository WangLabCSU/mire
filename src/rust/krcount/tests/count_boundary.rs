//! Count tables are available without R or a dependency on classification producers.
use std::fs;

use mire_krcount::{count_reads, CountRequest, Error};
use mire_kreport::{ReportError, ReportRequest};
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
                report: ReportRequest {
                    path: report.to_str().unwrap(),
                    taxonomy: None,
                },
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
    let report = dir.path().join("empty");
    fs::write(&report, "").unwrap();
    let result = count_reads(
        CountRequest {
            report: ReportRequest {
                path: report.to_str().unwrap(),
                taxonomy: None,
            },
            input: "unused",
            umi_tag: None,
            barcode_tag: None,
        },
        1,
        Some(1),
    );
    assert!(matches!(result, Err(Error::Report(ReportError::Empty(_)))));
}
