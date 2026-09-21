use bytes::Bytes;
use extendr_api::prelude::*;
use mire_koutput::{self as workflows, ExtractClassificationsRequest};
use mire_sequence::FastqPaths;
use mire_streaming::ProcessingOptions;

use super::values::strings_arg;

#[extendr]
fn kractor_reads(
    koutput: &str,
    fq1: &str,
    ofile1: Option<&str>,
    fq2: Option<&str>,
    ofile2: Option<&str>,
    compression_level: i32,
    batch_size: usize,
    chunk_bytes: usize,
    nqueue: Option<usize>,
    threads: usize,
) -> std::result::Result<(), String> {
    workflows::extract_reads(
        koutput,
        FastqPaths {
            input1: fq1,
            input2: fq2,
            output1: ofile1,
            output2: ofile2,
        },
        ProcessingOptions {
            compression_level,
            batch_size,
            chunk_bytes,
            nqueue,
            threads,
        },
    )
    .map_err(|error| error.to_string())
}

#[extendr]
fn kractor_koutput(
    kreport: &str,
    koutput: &str,
    taxonomy: Robj,
    ranks: Robj,
    taxa: Robj,
    taxids: Robj,
    exclude: Robj,
    descendants: bool,
    ofile: &str,
    compression_level: i32,
    batch_size: usize,
    chunk_bytes: usize,
    nqueue: Option<usize>,
    threads: usize,
) -> std::result::Result<(), String> {
    if [&taxonomy, &ranks, &taxa, &taxids, &exclude]
        .iter()
        .all(|value| value.is_null())
    {
        return Err(
            "One of 'taxonomy', 'ranks', 'taxa', 'taxids', 'exclude' must be provided".into(),
        );
    }
    let request = ExtractClassificationsRequest {
        report: kreport,
        taxonomy: strings_arg(&taxonomy, "taxonomy")?,
        input: koutput,
        output: ofile,
        ranks: strings_arg(&ranks, "ranks")?.map(|values| values.into_iter().collect()),
        names: strings_arg(&taxa, "taxa")?.map(|values| values.into_iter().collect()),
        taxids: strings_arg(&taxids, "taxids")?.map(|values| values.into_iter().collect()),
        descendants,
        excluded_lca: strings_arg(&exclude, "exclude")?
            .map(|values| values.into_iter().map(Bytes::from).collect()),
    };
    workflows::extract_classifications(
        request,
        ProcessingOptions {
            compression_level,
            batch_size,
            chunk_bytes,
            nqueue,
            threads,
        },
    )
    .map_err(|error| error.to_string())
}

#[cfg(feature = "bench")]
#[extendr]
fn pprof_kractor_koutput(
    kreport: &str,
    koutput: &str,
    taxonomy: Robj,
    ranks: Robj,
    taxa: Robj,
    taxids: Robj,
    exclude: Robj,
    descendants: bool,
    ofile: &str,
    compression_level: i32,
    batch_size: usize,
    chunk_bytes: usize,
    nqueue: Option<usize>,
    threads: usize,
    pprof_file: &str,
) -> std::result::Result<(), String> {
    super::profile::run(pprof_file, || {
        kractor_koutput(
            kreport,
            koutput,
            taxonomy,
            ranks,
            taxa,
            taxids,
            exclude,
            descendants,
            ofile,
            compression_level,
            batch_size,
            chunk_bytes,
            nqueue,
            threads,
        )
    })
}

#[cfg(feature = "bench")]
#[extendr]
fn pprof_kractor_reads(
    koutput: &str,
    fq1: &str,
    ofile1: Option<&str>,
    fq2: Option<&str>,
    ofile2: Option<&str>,
    compression_level: i32,
    batch_size: usize,
    chunk_bytes: usize,
    nqueue: Option<usize>,
    threads: usize,
    pprof_file: &str,
) -> std::result::Result<(), String> {
    super::profile::run(pprof_file, || {
        kractor_reads(
            koutput,
            fq1,
            ofile1,
            fq2,
            ofile2,
            compression_level,
            batch_size,
            chunk_bytes,
            nqueue,
            threads,
        )
    })
}

#[cfg(not(feature = "bench"))]
extendr_module! {
    mod kractor;
    fn kractor_koutput;
    fn kractor_reads;
}

#[cfg(feature = "bench")]
extendr_module! {
    mod kractor;
    fn kractor_koutput;
    fn kractor_reads;
    fn pprof_kractor_koutput;
    fn pprof_kractor_reads;
}
