use bytes::Bytes;
use extendr_api::prelude::*;
use mire_koutput::{self as workflows, JoinReadsRequest, ReadJoin};
use mire_streaming::ProcessingOptions;

use super::{sequence::robj_to_tag_ranges, values::strings_arg};

#[extendr]
fn koutput_reads(
    kreport: &str,
    koutput: &str,
    fq1: &str,
    fq2: Option<&str>,
    ofile: &str,
    taxonomy: Robj,
    // lca: Option<Vec<&str>>, // Only build for the specific LCA
    exclude: Robj,
    ranges1: Robj,
    ranges2: Robj,
    // polyn_threshold: usize,
    // phred_threshould: usize,
    koutput_batch: usize,
    fastq_batch: usize,
    chunk_bytes: usize,
    compression_level: i32,
    nqueue: Option<usize>,
    threads: usize,
) -> std::result::Result<(), String> {
    let request = JoinReadsRequest {
        report: kreport,
        taxonomy: strings_arg(&taxonomy, "taxonomy")?,
        koutput,
        input1: fq1,
        input2: fq2,
        output: ofile,
        koutput_batch,
        tags: ReadJoin {
            first_tags: robj_to_tag_ranges(&ranges1)
                .map_err(|error| format!("Invalid ranges1: {error:#}"))?,
            second_tags: robj_to_tag_ranges(&ranges2)
                .map_err(|error| format!("Invalid ranges2: {error:#}"))?,
        },
        excluded_lca: strings_arg(&exclude, "exclude")?
            .map(|values| values.into_iter().map(Bytes::from).collect()),
    };
    let options = ProcessingOptions {
        compression_level,
        batch_size: fastq_batch,
        chunk_bytes,
        nqueue,
        threads,
    };
    workflows::join_reads(request, options).map_err(|error| error.to_string())
}

#[cfg(feature = "bench")]
#[extendr]
fn pprof_koutput_reads(
    kreport: &str,
    koutput: &str,
    fq1: &str,
    fq2: Option<&str>,
    ofile: &str,
    taxonomy: Robj,
    exclude: Robj,
    ranges1: Robj,
    ranges2: Robj,
    koutput_batch: usize,
    fastq_batch: usize,
    chunk_bytes: usize,
    compression_level: i32,
    nqueue: Option<usize>,
    threads: usize,
    pprof_file: &str,
) -> std::result::Result<(), String> {
    super::profile::run(pprof_file, || {
        koutput_reads(
            kreport,
            koutput,
            fq1,
            fq2,
            ofile,
            taxonomy,
            exclude,
            ranges1,
            ranges2,
            koutput_batch,
            fastq_batch,
            chunk_bytes,
            compression_level,
            nqueue,
            threads,
        )
    })
}

#[cfg(not(feature = "bench"))]
extendr_module! {
    mod koutput_reads;
    fn koutput_reads;
}

#[cfg(feature = "bench")]
extendr_module! {
    mod koutput_reads;
    fn koutput_reads;
    fn pprof_koutput_reads;
}
