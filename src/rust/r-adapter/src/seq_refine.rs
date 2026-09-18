use super::actions::robj_to_seq_actions;
use extendr_api::prelude::*;
use mire_sequence::{self as workflows, FastqPaths};
use mire_streaming::ProcessingOptions;

#[extendr]
fn seq_refine(
    fq1: &str,
    ofile1: Option<&str>,
    fq2: Option<&str>,
    ofile2: Option<&str>,
    actions1: Robj,
    actions2: Robj,
    batch_size: usize,
    chunk_bytes: usize,
    compression_level: i32,
    nqueue: Option<usize>,
    threads: usize,
) -> std::result::Result<(), String> {
    let first = robj_to_seq_actions(&actions1)
        .map_err(|error| format!("Failed to parse actions1: {error:#}"))?;
    let second = robj_to_seq_actions(&actions2)
        .map_err(|error| format!("Failed to parse actions2: {error:#}"))?;
    workflows::refine_reads(
        FastqPaths {
            input1: fq1,
            input2: fq2,
            output1: ofile1,
            output2: ofile2,
        },
        first,
        second,
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
fn pprof_seq_refine(
    fq1: &str,
    ofile1: Option<&str>,
    fq2: Option<&str>,
    ofile2: Option<&str>,
    actions1: Robj,
    actions2: Robj,
    batch_size: usize,
    chunk_bytes: usize,
    compression_level: i32,
    nqueue: Option<usize>,
    threads: usize,
    pprof_file: &str,
) -> std::result::Result<(), String> {
    super::profile::run(pprof_file, || {
        seq_refine(
            fq1,
            ofile1,
            fq2,
            ofile2,
            actions1,
            actions2,
            batch_size,
            chunk_bytes,
            compression_level,
            nqueue,
            threads,
        )
    })
}

#[cfg(not(feature = "bench"))]
extendr_module! {
    mod seq_refine;
    fn seq_refine;
}

#[cfg(feature = "bench")]
extendr_module! {
    mod seq_refine;
    fn seq_refine;
    fn pprof_seq_refine;
}
