//! Read and k-mer counting.
//! This member consumes the report facade and classified-read files, and returns
//! Rust count tables without depending on the producer or a language adapter.
//!
//! ```no_run
//! use mire_krcount::{count_reads, CountRequest};
//! use mire_kreport::ReportRequest;
//! let counts = count_reads(CountRequest {
//!     report: ReportRequest { path: "sample.kreport", taxonomy: None },
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
mod table;
pub use error::{Error, Result};
pub use files::{count_reads, CountRequest};
pub use table::{CountColumn, CountTables};
