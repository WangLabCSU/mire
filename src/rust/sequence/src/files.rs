use std::path::Path;

use bytes::Bytes;
use mire_streaming::{ProcessingOptions, RecordSource, Result};
use rustc_hash::FxHashSet as HashSet;

use crate::{
    application::{ExtractReads, RefineReads},
    io::records::FastqSource,
    FastqPaths, ReadFragment, SubseqActions,
};

/// Open a single FASTQ stream or synchronized read pairs.
///
/// # Errors
/// Returns input errors. Reading records also validates FASTQ and paired IDs.
pub fn read_fastq(
    first: &Path,
    second: Option<&Path>,
) -> Result<impl RecordSource<Record = ReadFragment> + Send> {
    FastqSource::open(first, second)
}

/// Extract paired reads by ID, or copy all reads from a single-end input.
///
/// # Errors
/// Returns input, pairing, processing or output errors.
pub fn extract_reads(
    ids: HashSet<Bytes>,
    paths: FastqPaths<'_>,
    options: ProcessingOptions,
) -> Result<()> {
    let executor = options.executor()?;
    let source = paths.source()?;
    ExtractReads::new(ids).execute(source, &mut paths.sink(&options)?, &executor)
}

/// Apply sequence actions to FASTQ records, then write the transformed reads.
///
/// # Errors
/// Returns action validation, input, pairing, processing or output errors.
pub fn refine_reads(
    paths: FastqPaths<'_>,
    first: Option<SubseqActions>,
    second: Option<SubseqActions>,
    options: ProcessingOptions,
) -> Result<()> {
    let use_case = RefineReads::new(paths.input2.is_some(), first, second)?;
    let executor = options.executor()?;
    let source = paths.source()?;
    use_case.execute(source, &mut paths.sink(&options)?, &executor)
}
