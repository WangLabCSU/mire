use crate::{
    domain::{model::KrakenReport, taxonomy::TaxonomySelection},
    ReportError, Result,
};

pub(crate) trait KrakenReportSource {
    fn load(&self) -> Result<KrakenReport>;
    fn description(&self) -> String;
}

/// Read a report and apply the requested taxonomy selection.
pub(crate) fn read_report(
    source: &impl KrakenReportSource,
    taxonomy: Option<&[&str]>,
) -> Result<KrakenReport> {
    let selection = taxonomy.map(TaxonomySelection::parse).transpose()?;
    let report = source.load()?;
    if report.is_empty() {
        return Err(ReportError::Empty(source.description()));
    }
    if let Some(selection) = selection {
        Ok(selection.apply(report)?)
    } else {
        Ok(report)
    }
}
