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
              identical(parsed$taxids, list(character(), character(), "2",
                                            c("2", "10"), c("2", "10"))),
              !"minimizer_len" %in% names(parsed))
    write_lines(c("20\t1\t1\tU\t0\tunclassified", "", report), path("unclassified.tsv"))
    stopifnot(identical(call("read_kreport", path("unclassified.tsv"), NULL), parsed))
    leaf <- call("read_kreport", path("report.tsv"), "11")
    stopifnot(identical(leaf$taxid, "11"), identical(leaf$taxids, list(c("2", "10"))))
    write_lines("100\t4\t4\tU\t0\tunclassified", path("unclassified-only.tsv"))
    stopifnot(length(call("read_kreport", path("unclassified-only.tsv"), NULL)$taxid) == 0L,
              length(call("read_kreport", path("unclassified-only.tsv"), "999")$taxid) == 0L,
              length(call("read_kreport", path("report.tsv"), "999")$taxid) == 0L)
    stopifnot(identical(call("read_kreport", path("report.tsv"), character()), parsed))
    fails("read_kreport", path("missing-report.tsv"), character(),
          message = paste0("open '", path("missing-report.tsv"), "':"))
    report8 <- sub("\t([RDGS])\t", "\t20\t5\t\\1\t", report)
    write_lines(report8, path("report8.tsv"))
    parsed8 <- call("read_kreport", path("report8.tsv"), "G__Genus")
    stopifnot(identical(parsed8$taxid, c("10", "11", "12")),
              all(parsed8$minimizer_len == 20), all(parsed8$minimizer_n_unique == 5))

    # Interpret rank depths consistently in rows, ancestor columns and filters.
    for (eight in c(FALSE, TRUE)) {
        ranked <- sub("\tG\t", "\tG2\t", report, fixed = TRUE)
        if (eight) ranked <- sub("\t([RDGS])", "\t20\t5\t\\1", ranked)
        write_lines(ranked, path("ranked.tsv"))
        parsed_rank <- call("read_kreport", path("ranked.tsv"), "G2__Genus")
        stopifnot(identical(parsed_rank$rank, c("G2", "S", "S")),
                  identical(parsed_rank$ranks[[2L]], c("D", "G2")),
                  identical(parsed_rank$taxid, c("10", "11", "12")))
        for (taxonomy in c("G__Genus", "G0__Genus", "Genus", "G", "G0", "G2")) {
            stopifnot(identical(call("read_kreport", path("ranked.tsv"), taxonomy),
                                parsed_rank))
        }
        stopifnot(length(call("read_kreport", path("ranked.tsv"), "G1__Genus")$taxid) == 0L)
        named_leaf <- call("read_kreport", path("ranked.tsv"), "Species A")
        stopifnot(identical(named_leaf$taxid, "11"),
                  identical(named_leaf$taxids, list(c("2", "10"))))
        ranked_zero <- sub("\tG2\t", "\tG0\t", ranked, fixed = TRUE)
        write_lines(ranked_zero, path("ranked-zero.tsv"))
        parsed_zero <- call("read_kreport", path("ranked-zero.tsv"), "G0__Genus")
        stopifnot(identical(parsed_zero$rank, c("G", "S", "S")),
                  identical(parsed_zero$ranks[[2L]], c("D", "G")),
                  identical(parsed_zero$taxid, c("10", "11", "12")))
    }
    for (rank in c("X", "G256", "Gx", "G00", "G02", "G002")) {
        write_lines(c("", paste("100", "4", "4", rank, "10", "Genus", sep = "\t")),
                    path("invalid-rank.tsv"))
        fails("read_kreport", path("invalid-rank.tsv"), NULL, message = "line 2")
        expected_message <- if (rank == "X") "Invalid taxonomic rank" else "Invalid rank depth"
        fails("read_kreport", path("invalid-rank.tsv"), NULL, message = expected_message)
    }

    # Taxids use the same canonical decimal form in both report formats and selections.
    for (eight in c(FALSE, TRUE)) {
        prefix <- if (eight) "100\t4\t4\t20\t5\tG" else "100\t4\t4\tG"
        for (taxid in c("", "00", "02", "0002", "taxon-A", "+2", "-2", " 2", "2 ", "２")) {
            write_lines(c("", paste(prefix, taxid, "Genus", sep = "\t")), path("invalid-taxid.tsv"))
            fails("read_kreport", path("invalid-taxid.tsv"), NULL, message = "line 2")
            fails("read_kreport", path("invalid-taxid.tsv"), NULL, message = "taxid")
        }
        for (taxid in c("4294967295", "4294967296", "999999999999999999999999999")) {
            write_lines(paste(prefix, taxid, "Genus", sep = "\t"), path("long-taxid.tsv"))
            parsed_taxid <- call("read_kreport", path("long-taxid.tsv"), taxid)
            stopifnot(identical(parsed_taxid$taxid, taxid))
        }
    }
    # Invalid conditions fail before report input is opened in each R workflow.
    for (taxid in c("", "00", "02", "0002")) {
        message <- if (taxid == "") "Empty taxon specification." else "Invalid taxid"
        fails("read_kreport", path("missing-report.tsv"), taxid, message = message)
        fails("kractor_koutput", path("missing-report.tsv"), path("kraken.tsv"), taxid,
              NULL, NULL, NULL, NULL, FALSE, path("unused.tsv"), 1L, 1L, 32L, 1L, 1L,
              message = message)
        fails("koutput_reads", path("missing-report.tsv"), path("kraken.tsv"), path("r1.fq"), NULL,
              path("unused.tsv"), taxid, NULL, NULL, NULL, 1L, 1L, 2048L, 1L, 1L, 1L,
              message = message)
        fails("krcount", path("unused.tsv"), path("missing-report.tsv"), taxid, NULL, NULL, 1L, 1L,
              message = message)
    }

    # Each R entry point adds report paths without changing the underlying failure.
    write_lines(character(), path("empty-report.tsv"))
    write_lines("broken", path("malformed-report.tsv"))
    for (name in c("malformed-report.tsv", "missing-report.tsv")) {
        bad_report <- path(name)
        error <- .Call("wrap__read_kreport", bad_report, NULL, PACKAGE = "mire")$err
        stopifnot(!is.null(error), grepl(bad_report, error, fixed = TRUE))
        if (name == "malformed-report.tsv") {
            stopifnot(grepl("line 1", error, fixed = TRUE),
                      grepl("Invalid line with 1 fields; expected 6 or 8", error, fixed = TRUE))
        }
        fails("kractor_koutput", bad_report, path("kraken.tsv"), NULL,
              NULL, NULL, "11", NULL, FALSE, path("unused.tsv"), 1L, 1L, 32L, 1L, 1L,
              message = error)
        fails("koutput_reads", bad_report, path("kraken.tsv"), path("r1.fq"), NULL,
              path("unused.tsv"), NULL, NULL, NULL, NULL, 1L, 1L, 2048L, 1L, 1L, 1L,
              message = error)
        fails("krcount", path("unused.tsv"), bad_report, NULL, NULL, NULL, 1L, 1L,
              message = error)
    }

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

    # Reports without selected taxa produce empty results in every workflow.
    for (selection in list(list("empty-report.tsv", NULL), list("report.tsv", "999"))) {
        report_path <- path(selection[[1L]])
        taxonomy <- selection[[2L]]
        empty <- call("read_kreport", report_path, taxonomy)
        stopifnot(identical(names(empty), names(parsed)), all(lengths(empty) == 0L))
        call("kractor_koutput", report_path, path("kraken.tsv"), taxonomy,
             NULL, NULL, "11", NULL, FALSE, path("empty-selected.tsv"), 1L, 1L, 32L, 1L, 1L)
        stopifnot(length(read_lines(path("empty-selected.tsv"))) == 0L)
        call("koutput_reads", report_path, path("kraken.tsv"), path("r1.fq"), NULL,
             path("empty-joined.tsv"), taxonomy, NULL, NULL, NULL, 1L, 1L, 2048L, 1L, 1L, 1L)
        stopifnot(!file.exists(path("empty-joined.tsv")))
        empty_counts <- call("krcount", path("joined.tsv.gz"), report_path, taxonomy,
                             NULL, "BARCODE", 1L, 1L)
        stopifnot(identical(names(empty_counts), names(counts)),
                  all(lengths(empty_counts) == 0L))
    }

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
