//! File workflows exercise the public report and sequence interfaces together.
use std::fs;

use mire_koutput::{
    extract_classifications, join_reads, ExtractClassificationsRequest, JoinReadsRequest, ReadJoin,
};
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
            report: report.to_str().unwrap(),
            taxonomy: None,
            input: input.to_str().unwrap(),
            output: output.to_str().unwrap(),
            ranks: None,
            names: None,
            taxids: None,
            descendants: false,
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
            report: report.to_str().unwrap(),
            taxonomy: None,
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
    let report = dir.path().join("malformed");
    fs::write(&report, "broken\n").unwrap();
    let result = extract_classifications(
        ExtractClassificationsRequest {
            report: report.to_str().unwrap(),
            taxonomy: None,
            input: "unused",
            output: "unused",
            ranks: None,
            names: None,
            taxids: None,
            descendants: false,
            excluded_lca: None,
        },
        options(),
    );
    let error = result.unwrap_err();
    assert!(error.to_string().contains("line 1"));
    assert!(error.to_string().contains(report.to_str().unwrap()));
    let report_error = std::error::Error::source(&error).unwrap();
    assert_eq!(
        report_error.source().unwrap().to_string(),
        "Invalid line with 1 fields; expected 6 or 8"
    );
}

#[test]
fn reports_without_selected_entries_produce_no_classification_lines() {
    let dir = tempdir().unwrap();
    let report = dir.path().join("report");
    let input = dir.path().join("kraken");
    let output = dir.path().join("selected");
    fs::write(&input, "C\ta\t2\t4\t2:2\n").unwrap();
    for (contents, taxonomy) in [
        ("", None),
        ("100\t1\t1\tD\t2\tBacteria\n", Some(vec!["999".into()])),
    ] {
        fs::write(&report, contents).unwrap();
        extract_classifications(
            ExtractClassificationsRequest {
                report: report.to_str().unwrap(),
                taxonomy,
                input: input.to_str().unwrap(),
                output: output.to_str().unwrap(),
                ranks: None,
                names: None,
                taxids: None,
                descendants: false,
                excluded_lca: None,
            },
            options(),
        )
        .unwrap();
        assert!(fs::read(&output).unwrap().is_empty());
    }
}
