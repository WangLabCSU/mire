use super::{
    application as classification,
    domain::{joined::ReadJoin, output::ClassificationFilter},
};
use crate::{output::ClassifiedReadSink, Result};
use bytes::Bytes;
use mire_kreport::{ReportRequest, TaxonSelection};
use mire_sequence::{read_fastq, FastqPaths};
use mire_streaming::{ChunkWriter, LineSource, OrderedExecutor, ProcessingOptions, WorkflowError};
use rustc_hash::FxHashSet as HashSet;
use std::path::Path;
/// Extract paired reads by ID; preserve the legacy single-end pass-through policy.
///
/// # Errors
/// Returns errors opening the ID list, reading FASTQ, pairing or writing output.
pub fn extract_reads(
    koutput: &str,
    paths: FastqPaths<'_>,
    options: ProcessingOptions,
) -> Result<()> {
    let ids = classification::read_ids(&mut LineSource::open_plain(Path::new(koutput))?)?;
    Ok(mire_sequence::extract_reads(ids, paths, options)?)
}

/// Report selection and LCA exclusions for a Kraken output file.
pub struct ExtractClassificationsRequest<'a> {
    pub report: ReportRequest<'a>,
    pub input: &'a str,
    pub output: &'a str,
    pub selection: TaxonSelection,
    pub excluded_lca: Option<HashSet<Bytes>>,
}

/// Select Kraken output lines without changing their contents.
///
/// # Errors
/// Returns report selection, input, compression or output errors.
pub fn extract_classifications(
    request: ExtractClassificationsRequest<'_>,
    options: ProcessingOptions,
) -> Result<()> {
    let report = request.report.load()?;
    let filter = ClassificationFilter::new(
        report.select_taxids(&request.selection),
        request.excluded_lca,
    )
    .map_err(|error| WorkflowError::operation("build LCA exclusion matcher", error))?;
    let executor = options.executor()?;
    let source = LineSource::open(Path::new(request.input))?;
    let mut sink = ChunkWriter::open(
        Path::new(request.output),
        options.chunk_bytes,
        options.compression_level,
    )?;
    Ok(classification::extract_classifications(
        source, &mut sink, &executor, &filter,
    )?)
}

/// Inputs and tag extraction policy for joining classifications to reads.
pub struct JoinReadsRequest<'a> {
    pub report: ReportRequest<'a>,
    pub koutput: &'a str,
    pub input1: &'a str,
    pub input2: Option<&'a str>,
    pub output: &'a str,
    pub tags: ReadJoin,
    pub excluded_lca: Option<HashSet<Bytes>>,
    pub koutput_batch: usize,
}

/// Join classified reads and write the five-column classified-read format.
///
/// # Errors
/// Returns invalid report, read length, tag, pairing and I/O errors.
pub fn join_reads(request: JoinReadsRequest<'_>, options: ProcessingOptions) -> Result<()> {
    let report = request.report.load()?;
    let selected = report.select_taxids(&TaxonSelection {
        descendants: true,
        ..Default::default()
    });
    let filter = ClassificationFilter::new(selected, request.excluded_lca)
        .map_err(|error| WorkflowError::operation("build LCA exclusion matcher", error))?;
    let classifications = classification::load_classifications(
        LineSource::open(Path::new(request.koutput))?,
        &OrderedExecutor::new(request.koutput_batch, options.nqueue, options.threads)?,
        &filter,
    )?;
    if classifications.is_empty() {
        return Ok(());
    }
    let executor = options.executor()?;
    let source = read_fastq(Path::new(request.input1), request.input2.map(Path::new))?;
    let mut sink = ClassifiedReadSink {
        writer: ChunkWriter::open(
            Path::new(request.output),
            options.chunk_bytes,
            options.compression_level,
        )?,
        buffer: Vec::new(),
    };
    Ok(classification::join_reads(
        source,
        &mut sink,
        &executor,
        &classifications,
        &request.tags,
    )?)
}
