//! File workflows exercise the public report and sequence interfaces together.
use std::fs;

use mire_koutput::{
    extract_classifications, join_reads, ExtractClassificationsRequest, JoinReadsRequest, ReadJoin,
};
use mire_kreport::{ReportError, ReportRequest, TaxonSelection};
use mire_streaming::ProcessingOptions;
use tempfile::tempdir;

fn options() -> ProcessingOptions {
    ProcessingOptions {
        compression_level: 1,
        batch_size: 1,
        chunk_bytes: 64,
        nqueue: Some(0),
        threads: 2,
    }
}

#[test]
fn classification_selection_preserves_original_lines_and_order() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("report");
    let input = dir.path().join("kraken");
    let output = dir.path().join("selected");
    fs::write(&report, "100\t3\t3\tD\t2\tBacteria\n").unwrap();
    let selected = "C\ta\t2\t4\t2:2\nC\tc\t2\t4\t2:1 0:1\n";
    fs::write(
        &input,
        "C\ta\t2\t4\t2:2\nU\tb\t0\t4\t0:2\nC\tc\t2\t4\t2:1 0:1\n",
    )
    .unwrap();
    extract_classifications(
        ExtractClassificationsRequest {
            report: ReportRequest {
                path: report.to_str().unwrap(),
                taxonomy: None,
            },
            input: input.to_str().unwrap(),
            output: output.to_str().unwrap(),
            selection: TaxonSelection::default(),
            excluded_lca: None,
        },
        options(),
    )
    .unwrap();
    assert_eq!(fs::read_to_string(output).unwrap(), selected);
}

#[test]
fn paired_join_uses_sequence_facade_and_preserves_tag_precedence() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("report");
    let input = dir.path().join("kraken");
    let first = dir.path().join("first.fq");
    let second = dir.path().join("second.fq");
    let output = dir.path().join("joined");
    fs::write(&report, "100\t1\t1\t10\t5\tD\t2\tBacteria\n").unwrap();
    fs::write(&input, "C\ta\t2\t4:4\t2:2 |:| 2:2\n").unwrap();
    fs::write(&first, "@a MIRE{UMI:AC}\nACGT\n+\n1234\n").unwrap();
    fs::write(&second, "@a MIRE{UMI:TG}\nTGCA\n+\n5678\n").unwrap();
    join_reads(
        JoinReadsRequest {
            report: ReportRequest {
                path: report.to_str().unwrap(),
                taxonomy: None,
            },
            koutput: input.to_str().unwrap(),
            input1: first.to_str().unwrap(),
            input2: Some(second.to_str().unwrap()),
            output: output.to_str().unwrap(),
            tags: ReadJoin {
                first_tags: None,
                second_tags: None,
            },
            excluded_lca: None,
            koutput_batch: 1,
        },
        options(),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(output).unwrap(),
        "2\tUMI:TG\t2:2 |:| 2:2\tACGT TGCA\t1234 5678\n"
    );
}

#[test]
fn report_errors_remain_structured_at_classification_boundary() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("empty");
    fs::write(&report, "").unwrap();
    let result = extract_classifications(
        ExtractClassificationsRequest {
            report: ReportRequest {
                path: report.to_str().unwrap(),
                taxonomy: None,
            },
            input: "unused",
            output: "unused",
            selection: TaxonSelection::default(),
            excluded_lca: None,
        },
        options(),
    );
    assert!(matches!(
        result,
        Err(mire_koutput::Error::Report(ReportError::Empty(_)))
    ));
}
