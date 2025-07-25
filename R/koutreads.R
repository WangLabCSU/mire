#' Kraken2 Output Processing for FASTQ Files
#'
#' This function processes Kraken2 output and associated FASTQ files, extracting
#' relevant taxonomic information and preparing them for further downstream
#' analysis.
#'
#' @param kreport Path to the Kraken2 report file.
#' @param koutput Path to the Kraken2 output file.
#' @param reads A character vector of FASTQ file paths, either the original
#' reads used as input to Kraken2 or the classified output reads (recommended
#' for efficiency as they are smaller). Accepts one file for single-end or two
#' files for paired-end.
#'
#' **If only one file is used in Kraken2 to generate the koutput file, only the
#' second read sequence will be extracted to match the koutput's Lowest Common
#' Ancestor (LCA)**. This is commonly observed in data such as 10x Genomics,
#' where the first read contains only UMI and barcode information, while the
#' second read carries the biological sequence. You can always change the input
#' files to treat the actual biological read as read1 if needed.
#' @param tag_ranges1,tag_ranges2 A list of sequence ranges for extracting tags
#'   from the first/second read in single-end/paired-end data. If `NULL`, no
#'   additional tag extraction occurs for the first/second read. These ranges
#'   must be created by the `tag()` function, or be a named [`seq_range()`]
#'   object (in which case it will be automatically wrapped by `tag()`). Note
#'   that the tag embedded by [`seq_refine()`] in the description header will
#'   always be extracted.
#' @param ofile A character string. Path to the output file that will store the
#'   matched reads extracted based on Kraken2 classification. The output is
#'   compressed if the extension is `.gz`. This file contains only reads whose
#'   taxonomic assignments match the filtering criteria, such as `taxonomy`
#'   inclusion and `exclude` filters. Useful for downstream analysis like
#'   quantification of taxon-specific reads.
#' @param taxonomy A character vector. The set of taxonomic groups to include
#'   (default: `c("D__Bacteria", "D__Fungi", "D__Viruses")`). This defines the
#'   global taxa to consider. Only the descendants within these groups will be
#'   considered. If `NULL`, all taxa will be used.
#' @param exclude A character vector of taxids to exclude sequences from usage.
#'   Typically used to exclude the host taxid (e.g., `9606` for human) from the
#'   analysis. By default, this excludes human sequences (`"9606"`).
#' @param koutput_batch,fastq_batch Integer. Number of FASTQ records/Koutput
#'   lines to accumulate before dispatching a chunk to worker threads for
#'   processing. This controls the granularity of parallel work and affects
#'   memory usage and performance.
#'   Default is `r code_quote(KOUTPUT_BATCH, quote = FALSE)` for `koutput_batch`
#'   and `r code_quote(FASTQ_BATCH, quote = FALSE)` for `fastq_batch`.
#' @inheritParams seq_refine
#' @export
koutreads <- function(kreport, koutput, reads, ofile,
                      tag_ranges1 = NULL, tag_ranges2 = NULL,
                      taxonomy = c("D__Bacteria", "D__Fungi", "D__Viruses"),
                      exclude = c("9606"),
                      koutput_batch = NULL, fastq_batch = NULL,
                      chunk_bytes = NULL,
                      compression_level = 4L,
                      nqueue = NULL, threads = NULL, odir = NULL) {
    rust_koutreads(
        kreport = kreport, koutput = koutput, reads = reads, ofile = ofile,
        tag_ranges1 = tag_ranges1, tag_ranges2 = tag_ranges2,
        taxonomy = taxonomy,
        exclude = exclude,
        koutput_batch = koutput_batch,
        fastq_batch = fastq_batch,
        chunk_bytes = chunk_bytes,
        compression_level = compression_level,
        nqueue = nqueue,
        threads = threads,
        odir = odir
    )
}

rust_koutreads <- function(kreport, koutput, reads, ofile,
                           tag_ranges1 = NULL, tag_ranges2 = NULL,
                           taxonomy = c(
                               "D__Bacteria", "D__Fungi", "D__Viruses"
                           ),
                           exclude = c("9606"),
                           koutput_batch = NULL,
                           fastq_batch = NULL, chunk_bytes = NULL,
                           compression_level = 4L, nqueue = NULL,
                           threads = NULL,
                           odir = NULL, pprof = NULL) {
    assert_string(kreport, allow_empty = FALSE, allow_null = FALSE)
    assert_string(koutput, allow_empty = FALSE, allow_null = FALSE)
    reads <- as.character(reads)
    if (length(reads) < 1L || length(reads) > 2L) {
        cli::cli_abort("{.arg reads} must be of length 1 or 2")
    }
    if (is_scalar(reads)) {
        fq1 <- reads[[1L]]
        fq2 <- NULL
    } else {
        fq1 <- reads[[1L]]
        fq2 <- reads[[2L]]
    }
    assert_string(ofile, allow_empty = FALSE)
    tag_ranges1 <- check_tag_ranges(tag_ranges1)
    tag_ranges2 <- check_tag_ranges(tag_ranges2)
    if (!is.null(taxonomy)) {
        taxonomy <- as.character(taxonomy)
        taxonomy <- taxonomy[!is.na(taxonomy)]
        if (length(taxonomy) == 0L) taxonomy <- NULL
    }
    if (!is.null(exclude)) {
        exclude <- as.character(exclude)
        if (length(exclude) == 0L) exclude <- NULL
    }
    assert_number_whole(koutput_batch, min = 1, allow_null = TRUE)
    assert_number_whole(fastq_batch, min = 1, allow_null = TRUE)
    assert_number_whole(chunk_bytes, min = 1, allow_null = TRUE)
    assert_number_whole(compression_level, min = 1, max = 12)
    assert_number_whole(threads,
        min = 1, max = as.double(parallel::detectCores()),
        allow_null = TRUE
    )
    threads <- threads %||% min(3, parallel::detectCores())
    nqueue <- check_queue(nqueue, 3L, threads)
    assert_string(odir, allow_empty = FALSE, allow_null = TRUE)
    assert_string(pprof, allow_empty = FALSE, allow_null = TRUE)
    odir <- odir %||% getwd()
    dir_create(odir)
    koutput_batch <- koutput_batch %||% KOUTPUT_BATCH
    fastq_batch <- fastq_batch %||% FASTQ_BATCH
    chunk_bytes <- chunk_bytes %||% CHUNK_BYTES
    ofile <- file.path(odir, ofile)
    if (is.null(pprof)) {
        rust_call(
            "koutput_reads",
            kreport = kreport, koutput = koutput,
            fq1 = fq1, fq2 = fq2, ofile = ofile,
            taxonomy = taxonomy, exclude = exclude,
            ranges1 = tag_ranges1, ranges2 = tag_ranges2,
            koutput_batch = koutput_batch,
            fastq_batch = fastq_batch,
            chunk_bytes = chunk_bytes,
            compression_level = compression_level,
            nqueue = nqueue,
            threads = threads
        )
    } else {
        rust_call(
            "pprof_koutput_reads",
            kreport = kreport, koutput = koutput,
            fq1 = fq1, fq2 = fq2, ofile = ofile,
            taxonomy = taxonomy, exclude = exclude,
            ranges1 = tag_ranges1, ranges2 = tag_ranges2,
            koutput_batch = koutput_batch,
            fastq_batch = fastq_batch,
            chunk_bytes = chunk_bytes,
            compression_level = compression_level,
            nqueue = nqueue,
            threads = threads,
            pprof_file = file.path(odir, pprof)
        )
    }
    cli::cli_inform(c("v" = "Finished"))
}

#' @param tag An character label used to label the extracted content.
#' @inheritParams subseq_actions
#' @export
#' @rdname koutreads
tag <- function(tag, ranges) {
    assert_string(tag, allow_empty = FALSE, allow_null = FALSE)
    UseMethod("tag", ranges)
}

#' @export
tag.mire_seq_range <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("mire_tag", "mire_seq_range")
    )
}

#' @export
tag.mire_seq_ranges <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("mire_tag", "mire_seq_ranges")
    )
}

is_tag <- function(x) inherits(x, "mire_tag")

check_tag_ranges <- function(tag_ranges, arg = caller_arg(tag_ranges),
                             call = caller_env()) {
    if (is.null(tag_ranges)) {
        return(tag_ranges)
    }
    if (is_tag(tag_ranges)) {
        tag_ranges <- list(tag_ranges)
    } else if (is_range(tag_ranges)) {
        cli::cli_abort(c(
            "{.arg {arg}} cannot be a bare {.fn seq_range} object.",
            i = "Wrap it with {.fn tag} to associate it with a tag name, e.g., {.code tag('UMI', seq_range(1, 12))}."
        ))
    } else if (is.list(tag_ranges)) {
        nms <- names(tag_ranges)
        for (i in seq_along(tag_ranges)) {
            if (is_tag(tag_ranges[[i]])) {
                next
            } else if (is_range(tag_ranges[[i]])) {
                if (is.null(nms)) {
                    cli::cli_abort(
                        "Elements of {.arg {arg}} must be named if they are not already created by {.fn tag}.",
                        call = call
                    )
                }
                if (is.na(nms[[i]]) || nms[[i]] == "") {
                    cli::cli_abort(
                        "Tag name for element {i} of {.arg {arg}} cannot be empty or NA.",
                        call = call
                    )
                }
                tag_ranges[[i]] <- tag(nms[[i]], tag_ranges[[i]])
            } else {
                # Ensure every element in list is a valid range/action object
                cli::cli_abort(
                    c(
                        "{.arg {arg}} must be a list of tag objects created by {.fn tag}.",
                        i = "Each element must be created by {.fn tag}, or a named {.fn seq_range} that will be auto-wrapped."
                    ),
                    call = call
                )
            }
        }
    } else {
        cli::cli_abort(c(
            "{.arg {arg}} must be a list of tag objects created by {.fn tag}.",
            i = "Each element must be created by {.fn tag}, or a named {.fn seq_range} that will be auto-wrapped."
        ), call = call)
    }
    if (length(tag_ranges) == 0) tag_ranges <- NULL
    tag_ranges
}
