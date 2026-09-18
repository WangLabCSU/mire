use super::{application::CountReads, table::CountTables};
use crate::Result;
use mire_kreport::ReportRequest;
use mire_streaming::{LineSource, OrderedExecutor};
use std::path::Path;
/// Input and tag labels for read and k-mer counting.
pub struct CountRequest<'a> {
    pub report: ReportRequest<'a>,
    pub input: &'a str,
    pub umi_tag: Option<String>,
    pub barcode_tag: Option<String>,
}

/// Count reads and k-mers for each barcode and taxonomic ancestor.
///
/// # Errors
/// Returns report, input, tag extraction or k-mer validation errors.
pub fn count_reads(
    request: CountRequest<'_>,
    batch_size: usize,
    nqueue: Option<usize>,
) -> Result<CountTables> {
    let report = request.report.load()?;
    let use_case = CountReads::new(&report, request.umi_tag, request.barcode_tag);
    let counts = use_case.execute(
        LineSource::open(Path::new(request.input))?,
        &OrderedExecutor::new(batch_size, nqueue, 1)?,
    )?;
    Ok(CountTables::build(&report, &counts)?)
}
