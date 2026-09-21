use std::fs::File;
use std::path::Path;

use kreport::KrakenReportReader;
use mire_streaming::{LineSource, OrderedExecutor, WorkflowError};

use super::{application::CountReads, report::read_taxonomy, table::CountTables};
use crate::Result;

/// Input and tag labels for read and k-mer counting.
pub struct CountRequest<'a> {
    pub report: &'a str,
    pub taxonomy: Option<Vec<String>>,
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
    let filters = request
        .taxonomy
        .unwrap_or_default()
        .into_iter()
        .map(TryInto::try_into)
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| WorkflowError::operation("select taxonomy", error))?;
    let input = File::open(request.report)
        .map_err(|error| WorkflowError::operation(format!("open '{}'", request.report), error))?;
    let reader = KrakenReportReader::with_filters(filters, input);
    let report = read_taxonomy(reader).map_err(|error| {
        WorkflowError::operation(format!("kraken report '{}'", request.report), error)
    })?;
    let use_case = CountReads::new(report.ancestors, request.umi_tag, request.barcode_tag);
    let counts = use_case.execute(
        LineSource::open(Path::new(request.input))?,
        &OrderedExecutor::new(batch_size, nqueue, 1)?,
    )?;
    Ok(CountTables::build(
        &report.taxids,
        report.ranks,
        report.taxa,
        &counts,
    )?)
}
