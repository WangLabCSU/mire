# Rscript --vanilla tests/workflows-boundary.R /path/to/mire.so
args <- commandArgs(trailingOnly = TRUE)
if (length(args)) dyn.load(normalizePath(args[[1L]])) else library(mire)

local({
    work <- tempfile("mire-workflows-")
    dir.create(work)
    on.exit(unlink(work, recursive = TRUE))
    path <- function(name) file.path(work, name)
    call <- function(name, ...) {
        result <- .Call(paste0("wrap__", name), ..., PACKAGE = "mire")
        stopifnot(inherits(result, "extendr_result"))
        if (!is.null(result$err)) stop(result$err)
        result$ok
    }
    fails <- function(name, ..., message) {
        result <- .Call(paste0("wrap__", name), ..., PACKAGE = "mire")
        stopifnot(inherits(result, "extendr_result"), !is.null(result$err),
                  grepl(message, result$err, fixed = TRUE))
    }
    read_lines <- function(file) {
        con <- gzfile(file, "rt")
        on.exit(close(con))
        readLines(con)
    }
    write_lines <- function(lines, file) {
        con <- if (endsWith(file, ".gz")) gzfile(file, "wt") else file(file, "wt")
        on.exit(close(con))
        writeLines(lines, con)
    }
    report <- c("100\t4\t0\tR\t1\troot", "100\t4\t0\tD\t2\t  Bacteria",
                "100\t4\t0\tG\t10\t    Genus", "50\t2\t2\tS\t11\t      Species A",
                "50\t2\t2\tS\t12\t      Species B")
    write_lines(report, path("report.tsv"))
    kraken <- c("C\ta\t11\t40\t11:10", "C\tb\tSpecies B (taxid 12)\t40\t12:10",
                "C\tc\t99\t40\t99:10", "U\tu\t0\t40\t0:10")
    write_lines(kraken, path("kraken.tsv"))
    seq1 <- paste(rep("ACGT", 10L), collapse = "")
    seq2 <- paste(rep("GCTA", 10L), collapse = "")
    qual <- paste(rep("I", 40L), collapse = "")
    reads1 <- c("@a desc", seq1, "+", qual, "@b", seq2, "+", qual,
                "@c", seq1, "+", qual, "@u", seq2, "+", qual)
    reads2 <- c("@a", seq2, "+", qual, "@b", seq1, "+", qual,
                "@c", seq2, "+", qual, "@u", seq1, "+", qual)
    write_lines(reads1, path("r1.fq"))
    write_lines(reads2, path("r2.fq.gz"))

    # Preserve six/eight-column report schemas, row order, and taxonomy selection.
    parsed <- call("read_kreport", path("report.tsv"), NULL)
    stopifnot(identical(parsed$taxid, c("1", "2", "10", "11", "12")),
              !"minimizer_len" %in% names(parsed))
    report8 <- sub("\t([RDGS])\t", "\t20\t5\t\\1\t", report)
    write_lines(report8, path("report8.tsv"))
    parsed8 <- call("read_kreport", path("report8.tsv"), "G__Genus")
    stopifnot(identical(parsed8$taxid, c("10", "11", "12")),
              all(parsed8$minimizer_len == 20), all(parsed8$minimizer_n_unique == 5))

    # Preserve the extraction workflow's existing named-taxid exclusion behavior.
    call("kractor_koutput", path("report.tsv"), path("kraken.tsv"), NULL,
         NULL, NULL, "11", "999", FALSE, path("selected.tsv.gz"), 1L, 1L, 32L, 1L, 3L)
    stopifnot(identical(read_lines(path("selected.tsv.gz")), kraken[1:2]))
    call("kractor_koutput", path("report.tsv"), path("kraken.tsv"), NULL,
         "G", NULL, NULL, NULL, TRUE, path("descendants.tsv"), 1L, 1L, 32L, 0L, 3L)
    stopifnot(identical(read_lines(path("descendants.tsv")), kraken[1:2]))

    # Single and paired extraction retain input order across gzip members.
    call("kractor_reads", path("descendants.tsv"), path("r1.fq"), path("one.fq.gz"),
         NULL, NULL, 1L, 1L, 32L, 1L, 3L)
    stopifnot(identical(read_lines(path("one.fq.gz")), reads1))
    call("kractor_reads", path("descendants.tsv"), path("r1.fq"), path("pair1.fq.gz"),
         path("r2.fq.gz"), path("pair2.fq.gz"), 1L, 1L, 32L, 0L, 3L)
    stopifnot(identical(read_lines(path("pair1.fq.gz")), reads1[1:8]),
              identical(read_lines(path("pair2.fq.gz")), reads2[1:8]))

    tag <- function(name) structure(list(NULL, 4L), tag = name,
                                    class = c("mire_tag", "mire_seq_range"))
    # End-to-end join and counting, including barcode-specific ancestor counts.
    call("koutput_reads", path("report.tsv"), path("kraken.tsv"), path("r1.fq"), NULL,
         path("joined.tsv.gz"), NULL, NULL, list(tag("BARCODE")), NULL,
         1L, 1L, 2048L, 1L, 1L, 3L)
    joined <- read_lines(path("joined.tsv.gz"))
    stopifnot(length(joined) == 2L,
              identical(sub("\t.*", "", joined), c("11", "12")))
    counts <- call("krcount", path("joined.tsv.gz"), path("report.tsv"), "G__Genus",
                   NULL, "BARCODE", 1L, 0L)
    stopifnot(identical(names(counts), c("taxa", "counts", "kmer_total", "kmer_unique")),
              identical(names(counts$taxa), c("D", "G", "S")),
              identical(as.numeric(counts$counts$ACGT), c(1, 1, NA)),
              identical(as.numeric(counts$counts$GCTA), c(1, NA, 1)),
              identical(as.numeric(counts$kmer_total$ACGT), c(10, 10, NA)),
              identical(as.numeric(counts$kmer_unique$ACGT), c(4, 4, NA)))

    # Preserve the existing serialized-quality filter for paired reads.
    paired_kraken <- sub("40\t", "40:40\t", kraken[1:2])
    paired_kraken <- paste(paired_kraken, "|:| 11:10")
    write_lines(paired_kraken, path("paired-kraken.tsv"))
    call("koutput_reads", path("report.tsv"), path("paired-kraken.tsv"), path("r1.fq"),
         path("r2.fq.gz"), path("paired-joined.tsv"), NULL, NULL, list(tag("BARCODE")),
         list(tag("UMI")), 1L, 1L, 2048L, 1L, 1L, 3L)
    paired_counts <- call("krcount", path("paired-joined.tsv"), path("report.tsv"), "G__Genus",
                          "UMI", "BARCODE", 1L, 1L)
    stopifnot(length(paired_counts$counts) == 0L)

    # Source and domain failures return useful errors, including bounded queues.
    write_lines(reads2[1:4], path("short.fq"))
    fails("kractor_reads", path("descendants.tsv"), path("r1.fq"), path("bad1.fq"),
          path("short.fq"), path("bad2.fq"), 1L, 1L, 32L, 0L, 3L,
          message = "record count mismatch")
    call("kractor_reads", path("descendants.tsv"), path("r1.fq"), path("zero-batch.fq"),
         NULL, NULL, 1L, 0L, 0L, 1L, 2L)
    stopifnot(identical(read_lines(path("zero-batch.fq")), reads1))
    write_lines("broken", path("invalid.tsv"))
    fails("krcount", path("invalid.tsv"), path("report.tsv"), NULL, NULL, NULL,
          1L, 0L, message = "5 fields")
})
