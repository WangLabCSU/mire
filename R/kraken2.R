#' Run Kraken2 for Taxonomic Classification
#'
#' This function runs the Kraken2 classifier on one or two FASTQ files (single-
#' or paired-end).
#'
#' @param reads A character vector of FASTQ files used as input to Kraken2.
#' Can be one file (single-end) or two files (paired-end).
#' @param ... Additional arguments passed to `kraken2` command.
#' @param kreport A string of path to save kraken2 report.
#' @param koutput A string of path to save kraken2 output.
#' @param classified_out A string of path to save classified sequences, which
#' should be a fastq file.
#' @param unclassified_out A string of path to save unclassified sequences,
#' which should be a fastq file.
#' @param db Path to the Kraken2 database. You can download prebuilt databases
#'   from <https://benlangmead.github.io/aws-indexes/k2>, or build your own by
#'   following the instructions at
#'   <https://github.com/DerrickWood/kraken2/wiki/Manual#kraken-2-databases>.
#' @param kraken2 Optional. Path to the Kraken2 binary if not in the system
#' `PATH`.
#' @param odir A string of path to the output directory.
#' @return None. This function is called for its side effects.
#'   It produces the following output files in `odir` (or the working directory
#'   if `odir` is `NULL`):
#'   - `kreport`: Kraken2 classification report
#'   - `koutput`: Kraken2 raw classification output
#'   - `classified_out`: FASTQ file of classified reads
#'   - `unclassified_out`: FASTQ file of unclassified reads (if specified)
#' @export
kraken2 <- function(reads, ...,
                    kreport = "kraken_report.txt",
                    koutput = "kraken_output.txt",
                    classified_out = "classified.fq",
                    unclassified_out = NULL, db = NULL, odir = NULL,
                    threads = NULL, kraken2 = NULL) {
    assert_string(kraken2, allow_empty = FALSE, allow_null = TRUE)
    reads <- as.character(reads)
    if (length(reads) < 1L || length(reads) > 2L) {
        cli::cli_abort("{.arg reads} must be of length 1 or 2")
    }
    assert_string(odir, allow_empty = FALSE, allow_null = TRUE)
    odir <- path_trim(odir %||% getwd())
    dir_create(odir)

    assert_string(kreport, allow_empty = FALSE)
    if (!is.null(kreport)) kreport <- file.path(odir, kreport)

    assert_string(koutput, allow_empty = FALSE)
    if (!is.null(koutput)) koutput <- file.path(odir, koutput)

    # https://github.com/DerrickWood/kraken2/wiki/Manual
    # Usage of --paired also affects the --classified-out and
    # --unclassified-out options; users should provide a # character in
    # the filenames provided to those options, which will be replaced by
    # kraken2 with "_1" and "_2" with mates spread across the two files
    # appropriately.
    assert_string(classified_out, allow_empty = FALSE, allow_null = TRUE)
    if (!is.null(classified_out)) {
        if (!grepl("\\.(fq|fastq)$", classified_out, ignore.case = TRUE)) {
            cli::cli_abort("{.arg classified_out} must have a file extension {.field .fq} or {.field .fastq}")
        }
        if (length(reads) == 2L) {
            classified_out <- sub(
                "\\.(fq|fastq)$", "#.\\1",
                classified_out,
                ignore.case = TRUE
            )
        }
        classified_out <- file.path(odir, classified_out)
    }

    assert_string(unclassified_out, allow_empty = FALSE, allow_null = TRUE)
    if (!is.null(unclassified_out)) {
        if (!grepl("\\.(fq|fastq)$", unclassified_out, ignore.case = TRUE)) {
            cli::cli_abort("{.arg unclassified_out} must have a file extension {.field .fq} or {.field .fastq}")
        }
        if (length(reads) == 2L) {
            unclassified_out <- sub(
                "\\.(fq|fastq)$", "#.\\1",
                unclassified_out,
                ignore.case = TRUE
            )
        }
        unclassified_out <- file.path(odir, unclassified_out)
    }

    assert_string(db, allow_empty = FALSE, allow_null = TRUE)
    assert_number_whole(threads, min = 1, allow_null = TRUE)

    system2(
        kraken2 %||% "kraken2",
        c(
            if (length(reads) == 2L) "--paired",
            if (!is.null(db)) db <- sprintf("--db %s", db),
            sprintf("--threads %d", threads %||% parallel::detectCores()),
            if (!is.null(koutput)) sprintf("--output %s", koutput),
            if (!is.null(kreport)) sprintf("--report %s", kreport),
            if (!is.null(classified_out)) {
                sprintf("--classified-out %s", classified_out)
            },
            if (!is.null(unclassified_out)) {
                sprintf("--unclassified-out %s", unclassified_out)
            },
            "--use-names", "--report-minimizer-data",
            ...,
            reads
        )
    )
}

path_trim <- function(path) {
    # remove trailing backslash or slash
    sub("(\\\\+|/+)$", "", path, perl = TRUE)
}
