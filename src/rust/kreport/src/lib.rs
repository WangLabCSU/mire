//! Read Kraken reports and select taxonomic entries.
//!
//! Use [`load_kreport`] to collect a report from an input source, or
//! [`KrakenReportReader`] to read entries one at a time.
//! Both support six- and eight-column reports, preserve report order and retain
//! ancestor lineages without the current taxon or ancestors whose major rank is
//! root, such as `R`, `R1` and `R2`.
//! Blank and unclassified rows are skipped.
//! Create taxon conditions with [`TaxonSpec`] and combine them with
//! [`EntrySpec::new`] to match the entry's taxon or its ancestors.
//! Use [`EntrySpec::with_scope`] to choose a different matching scope. Inspect the
//! returned [`KrakenReport`] or [`KrakenReportEntry`] values. Reading failures return [`Error`].
//!
//! Read a report file and select bacteria and their descendants:
//!
//! ```no_run
//! use std::fs::File;
//!
//! use kreport::{load_kreport, EntrySpec, KrakenReport, TaxonSpec};
//!
//! let taxon_specs = [TaxonSpec::parse("D__Bacteria".into())?].into_iter().collect();
//! let entry_spec = EntrySpec::new(taxon_specs);
//! let report: KrakenReport = load_kreport(File::open("sample.kreport")?, entry_spec)?;
//! for entry in &report {
//!     println!("{}", entry.taxon().term());
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
//! while let Some(entry) = reader.read_entry()? {
//!     println!("{}", entry.taxon().term());
//! }
//! # Ok::<(), kreport::Error>(())
//! ```
#![deny(unreachable_pub)]

mod domain;
mod error;
mod reader;

pub use self::domain::{
    EntrySpec, EntrySpecScope, KrakenReport, KrakenReportEntry, KrakenReportEntryParts, ParseError,
    Rank, Taxid, Taxon, TaxonLevel, TaxonSpec, TaxonSpecParseError,
};
pub use self::error::Error;
pub use self::reader::{load_kreport, Entries, KrakenReportReader};
