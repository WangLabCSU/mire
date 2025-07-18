use aho_corasick::{AhoCorasick, AhoCorasickKind};
use anyhow::{anyhow, Context, Result};
use extendr_api::prelude::*;
use libdeflater::CompressionLvl;
use rustc_hash::FxHashMap as HashMap;
use rustc_hash::FxHashSet as HashSet;

mod koutput;
mod reads;

use crate::kreport::taxonomy_kreport;
use crate::seq_tag::robj_to_tag_ranges;
use crate::utils::*;

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
    koutput_reads_internal(
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
    .map_err(|e| format!("{:?}", e))
}

#[extendr]
#[cfg(feature = "bench")]
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
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(2000)
        .build()
        .with_context(|| format!("cannot create profile guard"))
        .map_err(|e| format!("{:?}", e))?;
    let out = koutput_reads(
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
    );
    if let Ok(report) = guard.report().build() {
        let file = std::fs::File::create(pprof_file)
            .with_context(|| format!("Failed to create file {}", pprof_file))
            .map_err(|e| format!("{:?}", e))?;
        let mut options = pprof::flamegraph::Options::default();
        options.image_width = Some(2500);
        report
            .flamegraph_with_options(file, &mut options)
            .with_context(|| format!("Failed to write flamegraph to {}", pprof_file))
            .map_err(|e| format!("{:?}", e))?;
    };
    out
}

fn koutput_reads_internal(
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
) -> Result<()> {
    let tag_ranges1 = robj_to_tag_ranges(&ranges1)?;
    let tag_ranges2 = robj_to_tag_ranges(&ranges2)?;
    let compression_level = CompressionLvl::new(compression_level)
        .map_err(|e| anyhow!("Invalid 'compression_level': {:?}", e))?;
    let exclude =
        robj_to_option_str(&exclude).with_context(|| format!("Failed to parse 'exclude'"))?;
    let kreports = taxonomy_kreport(kreport, taxonomy)?;

    // Build a map: taxid → set of its ancestor taxids
    let taxid_to_ancestors = kreports
        .iter()
        .map(|report| {
            // Each report's `taxids` field holds the lineage (ancestors) of this taxon
            let ancestors = report
                .taxids
                .iter()
                .map(|x| x.as_slice())
                .collect::<HashSet<&[u8]>>();
            // Map: current taxid → set of its ancestors
            (report.taxid.as_slice(), ancestors)
        })
        .collect::<HashMap<&[u8], HashSet<&[u8]>>>();

    // For each taxid, find all other taxids that consider it an ancestor
    // i.e., build a reverse lookup: taxid → set of descendant taxids
    let taxid_to_descendants = kreports
        .iter()
        .map(|report| {
            let taxid = report.taxid.as_slice();
            // Iterate over all taxids and check whose ancestor set includes the current taxid
            let descendants = taxid_to_ancestors
                .iter()
                .filter_map(|(child, parents)| {
                    if parents.contains(taxid) {
                        Some(*child)
                    } else {
                        None
                    }
                })
                .collect::<HashSet<&[u8]>>();

            // Map: current taxid → set of its descendant taxids
            (taxid, descendants)
        })
        .collect::<HashMap<&[u8], HashSet<&[u8]>>>();

    let include_sets = kreports
        .iter()
        // Always include the descendants
        .filter_map(|kr| taxid_to_descendants.get(kr.taxid.as_slice()))
        .flatten()
        .copied()
        .collect::<HashSet<&[u8]>>();

    // A space-delimited list indicating the LCA mapping of each
    // k-mer in the sequence(s). For example, "562:13 561:4 A:31 0:1 562:3" would indicate that:
    //
    // the first 13 k-mers mapped to taxonomy ID #562
    // the next 4 k-mers mapped to taxonomy ID #561
    // the next 31 k-mers contained an ambiguous nucleotide
    // the next k-mer was not in the database
    // the last 3 k-mers mapped to taxonomy ID #562
    let exclude_aho = exclude
        .map(|v| {
            let patterns: Vec<Vec<u8>> = v
                .iter()
                .map(|taxid| {
                    let mut pattern = Vec::with_capacity(taxid.len() + 1); // estimated capacity
                    pattern.extend_from_slice(taxid.as_bytes());
                    pattern.push(b':');
                    pattern
                })
                .collect();
            AhoCorasick::builder()
                .kind(Some(AhoCorasickKind::DFA))
                .build(patterns)
        })
        .transpose()?;

    // Read Kraken2 output and extract matched records
    let koutmap = koutput::parse_koutput(
        koutput,
        include_sets,
        exclude_aho,
        koutput_batch,
        nqueue,
        threads,
    )?;

    if koutmap.is_empty() {
        println!("No taxonomic matches found in the koutput file.");
        return Ok(());
    }

    // For each koutput row, we calculate kmer information
    reads::parse_reads(
        &koutmap,
        fq1,
        fq2,
        ofile,
        tag_ranges1,
        tag_ranges2,
        fastq_batch,
        chunk_bytes,
        compression_level,
        nqueue,
        threads,
    )?;
    Ok(())
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
