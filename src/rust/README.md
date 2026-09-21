# Rust boundaries

The Cargo workspace contains four business members: `kreport`,
`mire-sequence`, `mire-koutput`, and `mire-krcount`. Each can be built and tested
independently. There is no combined `mire-core` crate. The technical member
`mire-streaming` owns shared streaming ports and adapters. `mire-r`
(`r-adapter/`) converts R arguments/results and exports the existing `libmire.a`
entry points. `R/` remains the R package interface. Only `mire-r` depends on R,
extendr, anyhow, or profiling.

```text
R/ -> mire-r -> kreport
            -> mire-sequence -> mire-streaming
            -> mire-koutput  -> kreport, mire-sequence, mire-streaming
            -> mire-krcount  -> kreport, mire-streaming
            -> mire-streaming
```

Each business member publishes supported operations and Rust request/result
types through `lib.rs`. Other members cannot access its parsers, accumulators,
or internal record fields. File composition lives in the owning context's
`files.rs`; report services receive an input source opened by their caller.
FASTQ parsing and serialization belong to `mire-sequence`; classified-read
serialization belongs to `mire-koutput`. `mire-streaming` contains only generic
record ports, line/byte I/O, compression, scheduling, and progress. It has no
dependency on a business member and handles no biological format or policy.

| Boundary | Published behavior |
| --- | --- |
| `kreport` | Read and filter reports; expose entries, taxids, ranks, names and entry lineages |
| `mire-sequence` | Read validated FASTQ fragments, construct ranges/actions/tags, refine and extract reads |
| `mire-koutput` | Select classification lines, extract classified reads, join classifications and reads |
| `mire-krcount` | Count reads and k-mers by barcode and taxonomic ancestry |

## Reading Kraken reports

The two application services are `load_kreport` and `KrakenReportReader`.
Their inputs, results and errors are public types: use `TaxonSpec` for selection
conditions, `KrakenReport` and `KrakenReportEntry` for results, and `Error` for
reading failures.
Use `load_kreport` to collect a report from an input source. Pass an empty set of
conditions for all classified taxa, or parse conditions with `TaxonSpec::parse`
to include matching taxa and their descendants:

```rust,no_run
use std::fs::File;

use kreport::{load_kreport, KrakenReport, TaxonSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filters = [TaxonSpec::parse("D__Bacteria".into())?].into_iter().collect();
    let report: KrakenReport = load_kreport(File::open("sample.kreport")?, filters)?;
    for entry in report.entries() {
        println!("{}: {}", entry.taxon().taxid(), entry.taxon().term());
    }
    Ok(())
}
```

Use `KrakenReportReader` for an existing input source. `new` reads all classified
entries; `with_filters` accepts the same conditions:

```rust
use kreport::{KrakenReportReader, TaxonSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = b"100\t2\t2\tD\t2\tBacteria\n";
    let filters = [TaxonSpec::parse("D__Bacteria".into())?].into_iter().collect();
    let mut reader = KrakenReportReader::with_filters(filters, input.as_slice());
    while let Some(entry) = reader.read_entry()? {
        println!("{}", entry.taxon().term());
    }
    Ok(())
}
```

Use `reader.entries()` to obtain an `Entries` iterator over the remaining selected
entries. Each item is a `Result<KrakenReportEntry, Error>`; collect the remaining
entries with `reader.entries().collect::<Result<Vec<_>, _>>()?`.

Both support six- and eight-column reports and preserve report order. Blank and
unclassified rows are skipped. An entry's `lineage()` contains its taxon's ancestors,
excluding the taxon itself and root taxa. Ancestors remain in the lineage even
when they are not selected. Root entries are included in an unrestricted report
or when a condition matches the root.

Use `report.entries()` to inspect entries, or `report.taxids()`, `report.ranks()`
and `report.taxa()` to iterate identifiers, taxonomic levels and names.
Use `report.into_entries()` and `entry.into_parts()` when you need owned values,
including counts and lineages.

### Selecting taxa

Use `TaxonSpec::parse` or `try_into()` to create conditions for either service:

| Condition | Matches |
| --- | --- |
| `562` | The exact taxid |
| `Genus` | Genus and its intermediate levels |
| `G` or `G0` | Genus and its intermediate levels |
| `G2` | Taxa two levels below their genus ancestor |
| `Bacteria` | The exact scientific name |
| `G__Genus group` | That name at genus or an intermediate level below genus |
| `G2__Genus group` | That name exactly two levels below genus |

Report selection includes taxa matching any condition and their descendants.
Full rank names are case-sensitive. `Genus` selects a rank; use `G__Genus` to
select taxa named `Genus` at that rank or its intermediate levels.
Unrecognized levels and incomplete conditions, such as `G256`, `X__name` and
`G__`, are treated as scientific names. Empty condition strings (`""`) and numeric identifiers
with leading zeros, such as `02`, are rejected.

### Taxonomic values

Read identifiers with `entry.taxon().taxid()` and their text with `as_str()`.
`0` denotes unclassified reads. Decimal identifiers have no numeric upper bound;
`4294967296` is accepted. Leading zeros (`02`), signs (`+2`), whitespace (` 2`),
non-ASCII digits (`２`) and nonnumeric identifiers (`taxon-A`) are rejected.

Read a taxonomic level with `entry.taxon().level()`; `rank()` identifies its
major rank, such as genus. `rank().abbre()` gives its report abbreviation.
For `G2`, `rank()` is genus and `depth()` is the distance of two levels below
the nearest genus ancestor. `G` and `G0` denote genus itself, with no intermediate
level, and both display as `G`. Depths range from 0 to 255; signs (`G+1`) and
leading zeros (`G00`, `G02`) are rejected.

### Handling errors

Invalid conditions return `TaxonSpecParseError` before reading starts. Opening
an input file is handled by the caller. Report reading and parsing failures
return `Error::Read` or `Error::Parse`, with the report line. The latter carries
a `ParseError` describing the failed entry. Use `?` to propagate errors, match
variants for separate handling, and `std::error::Error::source()` to inspect causes.

An empty filter set includes all classified taxa. If no classified entries match,
`load_kreport` returns an empty report; check `report.is_empty()` for this outcome.
`KrakenReportReader::read_entry()` returns `Ok(None)` when no selected entries remain.
Taxonomy conditions do not suppress errors in unselected rows.

## Other workflows

`mire-koutput` owns rank, name and taxid selection. Supplied fields are intersected
before including descendants; an absent filter is unrestricted, while an empty
set matches nothing. Classification requests carry these fields directly.
`mire-krcount` builds its ancestry index once per counting operation and projects
lineage columns in biological rank order. Both workflows use the last entry's
lineage for duplicate taxids, as the previous report index did.

`mire-koutput` reads FASTQ through `mire_sequence::read_fastq` and uses read
accessors and `TagRanges::map_sequences`. Read fields remain private, and ID
extraction delegates FASTQ selection to `mire_sequence::extract_reads`.
`mire-krcount` consumes the classified-read file contract without depending on
the producer crate. Domain algorithms use domain values; applications depend
on record ports, with concrete I/O and executors selected in file adapters.

The R adapter owns R classes, 1-based range conversion, result column names,
optional profiling, and all extendr macros. Business APIs use Rust
requests/results; no business type implements conversions from `Robj`.

## Checks

Run from this directory:

```sh
cargo test -p kreport
cargo test -p mire-sequence -p mire-koutput -p mire-krcount -p mire-streaming
cargo fmt --all --check
cargo clippy --workspace --all-targets --features bench -- -D warnings
cargo test --workspace --features bench
cargo build -p mire-r --release
```

Only the adapter checks need R development/linker libraries. `isal` forwards
to `mire-streaming`'s optional compression adapter and needs the ISA-L build tools.
For native Unix builds, the R adapter probes the library directory with
`R --vanilla` and supplies both linker and runtime search paths. Ordinary
`cargo test --workspace --all-targets` therefore needs no manual
`LIBRARY_PATH` or `LD_LIBRARY_PATH`, even when an R startup profile prints messages.
Set `R_HOME` to select an installation explicitly, or make `R` available on `PATH`.
From the repository root, `sh tools/check-rust-link.sh --offline` verifies the
linker fix with noisy R profiles and a fresh temporary Cargo target directory.
The root default member remains `mire-r` for the existing R build scripts.
Both Makevars templates select that package explicitly; the static library
name and registered R function signatures are unchanged.

Also test the source-package installation with R's own C compiler and linker:

```sh
# From the repository root; installs into a temporary R library.
sh tools/check-install.sh
```

Cargo-only checks can miss R toolchain failures. For example, R built with
GCC 16 cannot compile the bundled C sources in `libdeflate-sys` 1.24.0 because
they use the removed `evex512` target attribute. `libdeflater` and
`libdeflate-sys` now require/resolve to 1.25.2 for that compatibility fix.

`ProcessingOptions::nqueue = Some(q)` limits work in flight to at most
`q + threads + 1` batches. Memory also includes the report/classification
indexes and output chunks; a single record can exceed the chunk target.
`None` preserves the explicitly unbounded queue option. Zero batch size means
one record at a time. Worker errors cancel blocked producers and propagate to
the caller; the sink reports flush errors explicitly.

## Compatibility audit

`tests/workflows-equivalence.R` (at the package root) runs the original and
refactored libraries in separate R processes. Build each as `mire.so` in its
own directory and run:

```sh
Rscript --vanilla tests/workflows-equivalence.R OLD/mire.so NEW/mire.so
```

The audit compares 125 cases: six/eight-column reports and taxonomy selection;
classification filters and LCA exclusions; single/paired extraction and
refinement; tag merging; classification/read joining; barcode/UMI, read and
k-mer counts; empty/malformed inputs; tolerated extra/unused fields; and zero
buffer/batch sizes. It compares R values and types with `identical()`, and
compares decompressed output lines. Single-worker output order is compared
exactly. Multi-worker FASTQ comparisons sort **whole records**, preserving the
association between header, sequence and quality. Error cases compare
success/failure, not the exact wording of contextual errors.

The baseline for this refactor is the working source snapshot taken before
this task's broad refactor, including the existing sequence/report changes;
it is not necessarily the repository's HEAD. The boundary extraction did not
upgrade external dependencies; the later R installation fix updates only
`libdeflater` and `libdeflate-sys`.

Member extraction is checked against the immediately preceding implementation
using strict comparison:

```sh
Rscript --vanilla tests/workflows-equivalence.R BEFORE/mire.so AFTER/mire.so --strict
```

This mode compares all 125 cases including output ordering and error text,
normalizing only temporary fixture paths. Business parsers, filtering,
counting, read transformations, and scheduling policies are preserved during
member extraction. This does not resolve the broader refactor's operational
differences described below. Rust integration tests exercise the published
member APIs, and compile-fail doctests check that internal types and fields
cannot be accessed across boundaries.

Existing behavior is deliberately retained, including these surprising rules:

- Report files and read-ID lists are plain text. Other existing compressed
  inputs retain the original gzip decoder policy.
- Single-end extraction emits all reads; paired extraction filters IDs.
- Named taxids in the extraction workflow follow the original rule when an
  explicit exclusion argument is supplied, including an empty vector.
- LCA exclusions match `taxid:` substrings, and count tags use substring lookup.
- Count quality filtering includes the paired separator, so conventional paired
  records fail the existing threshold. This refactor does not change that rule.
- Short-read complexity arithmetic follows the original release-build behavior;
  explicit wrapping avoids the previous debug/release discrepancy.

The audit does **not** establish complete byte-for-byte equivalence: gzip chunk
boundaries can differ, concurrent output now follows input order, and errors
carry different context. Malformed tag annotations and overflowing k-mer counts
return errors instead of hanging or panicking. Duplicate classification IDs now
resolve to the last input occurrence, while old parallel scheduling could pick
a different occurrence. These operational differences must not be mistaken for
proof that every possible input has identical behavior.

There is also a performance tradeoff: compression currently runs in the ordered
output adapter. A local synthetic 100,000-read, four-worker refinement run
(152 bases/read, batch 256, 1 MiB chunks, level 1) took about 0.05 s plain and
0.07 s gzip, versus about 0.03 s for each in the baseline. These short timings
are only a regression indicator, not a representative throughput guarantee.
