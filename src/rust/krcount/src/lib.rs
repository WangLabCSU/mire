//! Read and k-mer counting.
//!
//! Count reads and k-mers from classified-read files by barcode and taxonomic
//! ancestry, using a Kraken report to identify each taxon's ancestors.
//!
//! ```no_run
//! use mire_krcount::{count_reads, CountRequest};
//! let counts = count_reads(CountRequest {
//!     report: "sample.kreport",
//!     taxonomy: None,
//!     input: "classified.tsv", umi_tag: None, barcode_tag: None,
//! }, 256, Some(2))?;
//! # Ok::<(), mire_krcount::Error>(())
//! ```
//!
//! ```compile_fail
//! use mire_krcount::domain::statistics::BarcodeCounts;
//! ```
mod application;
mod domain;
mod error;
mod files;
mod report;
mod table;

pub use self::error::{Error, Result};
pub use self::files::{count_reads, CountRequest};
pub use self::table::{CountColumn, CountTables};
