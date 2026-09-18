use std::{io, path::PathBuf};

use thiserror::Error;

use crate::{ParseError, TaxonomyError};

/// Structured failures at the report boundary, retaining file and line context.
#[derive(Debug, Error)]
pub enum ReportError {
    #[error("open '{}': {source}", .path.display())]
    Open {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("read '{}' at line {line}: {source}", .path.display())]
    Read {
        path: PathBuf,
        line: usize,
        #[source]
        source: io::Error,
    },
    #[error("line {line} of kraken report '{}': {source}", .path.display())]
    InvalidLine {
        path: PathBuf,
        line: usize,
        #[source]
        source: ParseError,
    },
    #[error("select taxonomy: {0}")]
    Taxonomy(#[from] TaxonomyError),
    #[error(
        "No entries found in kreport file: '{0}'. Please ensure it is not empty or malformed."
    )]
    Empty(String),
}

/// The result of reading or selecting a Kraken report.
pub type Result<T> = std::result::Result<T, ReportError>;
