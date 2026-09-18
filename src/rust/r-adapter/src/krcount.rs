use super::values::strings_arg;
use extendr_api::prelude::*;
use mire_krcount::{self as workflows, CountRequest};
use mire_kreport::ReportRequest;

#[extendr]
fn krcount(
    koutreads: &str,
    kreport: &str,
    taxonomy: Robj,
    umi_tag: Option<&str>,
    barcode_tag: Option<&str>,
    batch_size: usize,
    nqueue: Option<usize>,
) -> std::result::Result<List, String> {
    let request = CountRequest {
        report: ReportRequest {
            path: kreport,
            taxonomy: strings_arg(&taxonomy, "taxonomy")?,
        },
        input: koutreads,
        umi_tag: umi_tag.map(str::to_owned),
        barcode_tag: barcode_tag.map(str::to_owned),
    };
    let table =
        workflows::count_reads(request, batch_size, nqueue).map_err(|error| error.to_string())?;
    Ok(list![
        taxa = List::from_names_and_values(table.ranks, table.taxa)
            .map_err(|error| error.to_string())?,
        counts = List::from_names_and_values(table.barcodes.clone(), table.counts)
            .map_err(|error| error.to_string())?,
        kmer_total = List::from_names_and_values(table.barcodes.clone(), table.kmer_total)
            .map_err(|error| error.to_string())?,
        kmer_unique = List::from_names_and_values(table.barcodes, table.kmer_unique)
            .map_err(|error| error.to_string())?,
    ])
}

extendr_module! {
    mod krcount;
    fn krcount;
}
