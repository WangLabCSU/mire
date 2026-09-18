//! Kraken classifications and joining classifications to reads.
//! Use the requests and file operations exported here. Parsers, filters and
//! workflow coordination are private to this member.
//!
//! ```compile_fail
//! use mire_koutput::domain::output::Classification;
//! ```
mod application;
mod domain;
mod error;
mod files;
mod output;
pub use domain::joined::ReadJoin;
pub use error::{Error, Result};
pub use files::{
    extract_classifications, extract_reads, join_reads, ExtractClassificationsRequest,
    JoinReadsRequest,
};

pub(crate) use domain::joined::ClassifiedRead;
