# Rust boundaries

The Cargo workspace contains four business members: `mire-kreport`,
`mire-sequence`, `mire-koutput`, and `mire-krcount`. Each can be built and tested
independently. There is no combined `mire-core` crate. The technical member
`mire-streaming` owns shared streaming ports and adapters. `mire-r`
(`r-adapter/`) converts R arguments/results and exports the existing `libmire.a`
entry points. `R/` remains the R package interface. Only `mire-r` depends on R,
extendr, anyhow, or profiling.

```text
R/ -> mire-r -> mire-kreport
            -> mire-sequence -> mire-streaming
            -> mire-koutput  -> mire-kreport, mire-sequence, mire-streaming
            -> mire-krcount  -> mire-kreport, mire-streaming
            -> mire-streaming
```

Each business member owns private domain and application modules. Its `lib.rs`
publishes supported operations and Rust request/result types. Other members
cannot access its parsers, accumulators, or internal record fields. File
composition lives in the owning context's `files.rs` (reports use `file.rs`).
FASTQ parsing and serialization belong to `mire-sequence`; classified-read
serialization belongs to `mire-koutput`. `mire-streaming` contains only generic
record ports, line/byte I/O, compression, scheduling, and progress. It has no
dependency on a business member and handles no biological format or policy.

| Boundary | Published behavior |
| --- | --- |
| `mire-kreport` | Read a report, select taxids, query ancestors, project lineage columns and report rows |
| `mire-sequence` | Read validated FASTQ fragments, construct ranges/actions/tags, refine and extract reads |
| `mire-koutput` | Select classification lines, extract classified reads, join classifications and reads |
| `mire-krcount` | Count reads and k-mers by barcode and taxonomic ancestry |

`mire-kreport` owns the parser, builder, taxon entities, lineage index, selection
rules, private file reader and structured `ReportError`. It depends only on
`bytes`, `memchr`, `rustc-hash` and `thiserror`; it has no reverse dependency on
any other member. Counting and extraction call its facade; neither constructs report
entities nor rebuilds lineages. `ReportRow` and `TaxonLabel` are output
projections for callers to adapt, including languages other than R.

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
cargo test -p mire-kreport
cargo test -p mire-sequence -p mire-koutput -p mire-krcount -p mire-streaming
cargo fmt --all --check
cargo clippy --workspace --all-targets --features bench -- -D warnings
cargo test --workspace --features bench
cargo build -p mire-r --release
```

Only the adapter checks need R development/linker libraries. `isal` forwards
to `mire-streaming`'s optional compression adapter and needs the ISA-L build tools.
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
