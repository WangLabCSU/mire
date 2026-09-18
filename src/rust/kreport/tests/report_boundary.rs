//! Exercise only the published report interface, without R or private models.
use std::io::Write;

use mire_kreport::{read, ParseError, ReportError, TaxonSelection, TaxonomyError};
use tempfile::NamedTempFile;

const REPORT: &str = "100\t4\t0\tR\t1\troot\n100\t4\t0\tD\t2\t  Bacteria\n100\t4\t0\tG\t10\t    Genus\n50\t2\t2\tS\t11\t      Species A\n50\t2\t2\tS\t12\t      Species B\n";

#[test]
fn report_owns_selection_ancestry_and_projections() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(REPORT.as_bytes()).unwrap();
    let report = read(file.path(), Some(&["G__Genus"])).unwrap();
    assert_eq!(report.taxids().collect::<Vec<_>>(), ["10", "11", "12"]);
    let selected = report.select_taxids(&TaxonSelection {
        ranks: Some(["G".to_owned()].into_iter().collect()),
        descendants: true,
        ..Default::default()
    });
    assert_eq!(selected.len(), 3);
    assert!(selected.contains(b"11".as_slice()));
    let ancestors = report.ancestors(b"11").unwrap();
    assert!(ancestors.contains(b"2".as_slice()));
    assert!(ancestors.contains(b"10".as_slice()));
    assert!(!ancestors.contains(b"1".as_slice()));
    let (ranks, columns) = report.lineage_columns();
    assert_eq!(ranks, ["D", "G", "S"]);
    assert_eq!(
        columns[2],
        [None, Some("Species A".into()), Some("Species B".into())]
    );
    let rows = report.into_rows();
    assert_eq!(rows[1].taxon.taxid, "11");
    assert_eq!(rows[1].clade_reads, 2);
    assert_eq!(rows[1].minimizer_count, None);
}

#[test]
fn eight_column_projection_preserves_minimizer_units() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"100\t4\t2\t30\t7\tD\t2\tBacteria\n")
        .unwrap();
    let rows = read(file.path(), None).unwrap().into_rows();
    assert_eq!(rows[0].minimizer_count, Some(30));
    assert_eq!(rows[0].distinct_minimizer_count, Some(7));
}

#[test]
fn report_errors_identify_file_and_line() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"100\t4\t0\t\t1\troot\n").unwrap();
    let error = read(file.path(), None).err().unwrap();
    assert!(matches!(
        &error,
        ReportError::InvalidLine { path, line: 1, source: ParseError::MissingRank }
            if path == file.path()
    ));
    let message = error.to_string();
    assert!(message.contains("line 1"));
    assert!(message.contains(file.path().to_str().unwrap()));
    assert!(message.contains("Missing rank code"));
}

#[test]
fn report_boundary_preserves_structured_selection_and_empty_errors() {
    let mut file = NamedTempFile::new().unwrap();
    assert!(matches!(
        read(file.path(), None),
        Err(ReportError::Empty(_))
    ));
    file.write_all(REPORT.as_bytes()).unwrap();
    assert!(matches!(
        read(file.path(), Some(&[])),
        Err(ReportError::Taxonomy(TaxonomyError::EmptySelection))
    ));
    assert!(matches!(
        read(file.path(), Some(&["999"])),
        Err(ReportError::Taxonomy(TaxonomyError::NoMatches(_)))
    ));
}

#[test]
fn report_file_preserves_line_endings_and_mixed_column_formats() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b" \r\n100\t4\t0\tR\t1\troot\r\n100\t4\t2\t30\t7\tD\t2\t  Bacteria")
        .unwrap();
    let rows = read(file.path(), None).unwrap().into_rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].taxon.name, "root");
    assert_eq!(rows[0].minimizer_count, None);
    assert_eq!(rows[1].taxon.name, "Bacteria");
    assert_eq!(rows[1].minimizer_count, Some(30));
    assert_eq!(rows[1].lineage.len(), 1);
}

#[test]
fn report_file_preserves_unterminated_final_carriage_return() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(b"100\t4\t0\tD\t2\tBacteria\r").unwrap();
    let rows = read(file.path(), None).unwrap().into_rows();
    assert_eq!(rows[0].taxon.name, "Bacteria\r");
}
