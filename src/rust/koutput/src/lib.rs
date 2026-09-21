//! Kraken classifications and joining classifications to reads.
//!
//! Select Kraken output lines by taxon, extract reads by ID, and join
//! classifications with sequences, qualities and tags.
//!
//! ```compile_fail
//! use mire_koutput::domain::output::Classification;
//! ```
mod application;
mod domain;
mod error;
mod files;
mod output;
mod report;

pub use self::domain::joined::ReadJoin;
pub use self::error::{Error, Result};
pub use self::files::{
    extract_classifications, extract_reads, join_reads, ExtractClassificationsRequest,
    JoinReadsRequest,
};

pub(crate) use self::domain::joined::ClassifiedRead;
