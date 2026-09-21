//! Sequence ranges, tags and read refinement.
//!
//! Read FASTQ fragments, extract tags, trim sequence ranges and write refined reads.
//!
//! ```no_run
//! use mire_streaming::RecordSource;
//! let mut reads = mire_sequence::read_fastq("reads.fq".as_ref(), None)?;
//! while let Some(fragment) = reads.next_record()? {
//!     assert_eq!(fragment.first().sequence().len(), fragment.first().quality().len());
//! }
//! # Ok::<(), mire_streaming::WorkflowError>(())
//! ```
//!
//! ```compile_fail
//! use mire_sequence::ReadFragment;
//! fn replace_id(fragment: &mut ReadFragment) { fragment.read1.id = "changed".into(); }
//! ```
mod application;
mod domain;
mod files;
mod io;
mod paths;
pub use domain::{
    actions::{SeqAction, SeqActionError, SubseqActions, SubseqActionsBuilder},
    fragment::{Read, ReadFragment},
    range::{SeqRange, SeqRangeError, SeqRanges},
    record::FastqRecord,
    tag::{TagRangeError, TagRanges, TaggedSequences, TAG_PREFIX, TAG_SUFFIX},
};
pub use files::{extract_reads, read_fastq, refine_reads};
pub use paths::FastqPaths;

pub(crate) use domain::fragment::ReadPairError;
