//! Read Kraken reports and select taxa with their descendants.
//!
//! Use [`load_kreport`] to collect a report from an input source, or
//! [`KrakenReportReader`] to read entries one at a time.
//! Both support six- and eight-column reports, preserve report order and retain
//! ancestor lineages. Blank and unclassified rows are skipped.
//! Create selection conditions with [`TaxonSpec`] and inspect the returned
//! [`KrakenReport`] or [`KrakenReportEntry`] values. Reading failures return [`Error`].
//!
//! Read a report file and select bacteria and their descendants:
//!
//! ```no_run
//! use std::fs::File;
//!
//! use kreport::{load_kreport, KrakenReport, TaxonSpec};
//!
//! let filters = [TaxonSpec::parse("D__Bacteria".into())?].into_iter().collect();
//! let report: KrakenReport = load_kreport(File::open("sample.kreport")?, filters)?;
//! for taxon in report.taxa() {
//!     println!("{taxon}");
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Read entries from an existing source:
//!
//! ```
//! use kreport::KrakenReportReader;
//! let input = b"100\t2\t2\tD\t2\tBacteria\n";
//! let mut reader = KrakenReportReader::new(input.as_slice());
//! while let Some(entry) = reader.read_entry() {
//!     println!("{}", entry?.taxon().term());
//! }
//! # Ok::<(), kreport::Error>(())
//! ```
#![deny(unreachable_pub)]

mod domain;
mod error;
mod reader;

pub use self::domain::{
    KrakenReport, KrakenReportEntry, KrakenReportEntryParts, ParseError, Rank, Taxid, Taxon,
    TaxonLevel, TaxonSpec, TaxonSpecParseError,
};
pub use self::error::Error;
pub use self::reader::{load_kreport, KrakenReportReader};
