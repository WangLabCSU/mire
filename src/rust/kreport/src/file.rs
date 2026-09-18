use std::{
    fs::File,
    path::{Path, PathBuf},
};

use crate::{
    application::KrakenReportSource,
    domain::{builder::KrakenReportBuilder, model::KrakenReport, parser::KrakenReportParser},
    reader::LineReader,
    ReportError, Result,
};

const BUFFER_SIZE: usize = 4 * 1024 * 1024;

pub(crate) struct KrakenReportFile {
    path: PathBuf,
}

impl KrakenReportFile {
    pub(crate) fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
        }
    }
}

impl KrakenReportSource for KrakenReportFile {
    fn load(&self) -> Result<KrakenReport> {
        let file = File::open(&self.path).map_err(|source| ReportError::Open {
            path: self.path.clone(),
            source,
        })?;
        let mut reader = LineReader::with_capacity(BUFFER_SIZE, file);
        let mut builder = KrakenReportBuilder::new();
        let mut line_number = 0;
        while let Some(line) = reader.read_line().map_err(|source| ReportError::Read {
            path: self.path.clone(),
            line: reader.offset() + 1,
            source,
        })? {
            line_number += 1;
            let invalid_line = |source| ReportError::InvalidLine {
                path: self.path.clone(),
                line: line_number,
                source,
            };
            if let Some(row) = KrakenReportParser::parse_line(&line).map_err(invalid_line)? {
                builder.push(row).map_err(invalid_line)?;
            }
        }
        Ok(builder.finish())
    }

    fn description(&self) -> String {
        self.path.display().to_string()
    }
}

#[cfg(test)]
fn parse_kreport(path: impl AsRef<Path>) -> Result<Vec<crate::domain::model::KrakenReportEntry>> {
    crate::application::read_report(&KrakenReportFile::new(path), None)
        .map(KrakenReport::into_entries)
}

#[cfg(test)]
fn taxonomy_kreport(
    path: impl AsRef<Path>,
    taxonomy: &Option<Vec<&str>>,
) -> Result<Vec<crate::domain::model::KrakenReportEntry>> {
    crate::application::read_report(&KrakenReportFile::new(path), taxonomy.as_deref())
        .map(KrakenReport::into_entries)
}
#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::{parse_kreport, taxonomy_kreport};

    fn report_file(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        file
    }

    #[test]
    fn reads_report_from_file() {
        let file = report_file(
            "100.00\t10\t0\tR\t1\troot\n\
             100.00\t10\t1\tD\t2\t  Bacteria\n",
        );

        let entries = parse_kreport(file.path()).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].taxon().name(), "Bacteria");
    }

    #[test]
    fn selects_taxonomy_and_descendants() {
        let file = report_file(
            "100.00\t10\t0\tR\t1\troot\n\
             100.00\t10\t1\tD\t2\t  Bacteria\n\
             50.00\t5\t5\tS\t562\t    Escherichia coli\n",
        );
        let taxonomy = Some(vec!["D__Bacteria"]);

        let entries = taxonomy_kreport(file.path(), &taxonomy).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].taxon().name(), "Bacteria");
        assert_eq!(entries[1].taxon().name(), "Escherichia coli");
    }

    #[test]
    fn selects_taxonomy_by_taxid() {
        let file = report_file(
            "100.00\t10\t0\tR\t1\troot\n\
             100.00\t10\t1\tD\t2\t  Bacteria\n\
             50.00\t5\t5\tS\t562\t    Escherichia coli\n",
        );
        let taxonomy = Some(vec!["2"]);

        let entries = taxonomy_kreport(file.path(), &taxonomy).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].taxon().taxid(), "2");
        assert_eq!(entries[1].taxon().taxid(), "562");
    }

    #[test]
    fn rejects_empty_report() {
        let file = report_file("");

        let error = taxonomy_kreport(file.path(), &None).expect_err("expected an error");

        assert!(error.to_string().contains("No entries found"));
    }
}
#[cfg(test)]
mod error_tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::{KrakenReportFile, KrakenReportSource};

    #[test]
    fn adds_file_context_to_parser_errors() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"100.00\t10\t0\t\t1\troot\n").unwrap();
        let source = KrakenReportFile::new(file.path());

        let error = source
            .load()
            .expect_err("expected malformed report to fail");
        let message = error.to_string();

        assert!(message.contains(&file.path().display().to_string()));
        assert!(message.contains("Missing rank code"));
    }
}
