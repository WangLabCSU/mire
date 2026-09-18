# Run against the installed package, or pass a freshly linked mire shared library:
# Rscript --vanilla tests/sequence-boundary.R /path/to/mire.so
args <- commandArgs(trailingOnly = TRUE)
if (length(args)) {
    dyn.load(normalizePath(args[[1L]]))
} else {
    library(mire)
}

local({
    work <- tempfile("mire-sequence-")
    dir.create(work)
    on.exit(unlink(work, recursive = TRUE))
    input1 <- file.path(work, "read1.fq")
    input2 <- file.path(work, "read2.fq")
    output1 <- file.path(work, "out1.fq")
    output2 <- file.path(work, "out2.fq.gz")
    writeLines(c("@read description", "ACGT", "+", "1234"), input1)
    writeLines(c("@read", "TGCA", "+", "5678"), input2)

    seq_range <- function(start = NULL, end = NULL) {
        structure(list(start, end), class = "mire_seq_range")
    }
    action <- function(range, kind = "mire_embed_trim", tag = "UMI") {
        structure(range, class = c(kind, "mire_seq_action", class(range)), tag = tag)
    }
    refine <- function(actions1, actions2 = NULL, paired = FALSE,
                       input = input1, output = output1) {
        .Call("wrap__seq_refine", input, output,
              if (paired) input2 else NULL, if (paired) output2 else NULL,
              actions1, actions2, 1L, 1024L, 1L, 2L, 1L, PACKAGE = "mire")
    }
    succeeds <- function(result) {
        stopifnot(inherits(result, "extendr_result"), is.null(result$err))
    }
    fails <- function(result, message) {
        stopifnot(inherits(result, "extendr_result"),
                  !is.null(result$err), grepl(message, result$err, fixed = TRUE))
    }
    read_fastq <- function(path) {
        connection <- gzfile(path, "rt")
        on.exit(close(connection))
        readLines(connection)
    }

    # R bounds are one-based and inclusive, including single-base ranges.
    succeeds(refine(list(action(seq_range(2L, 3L)))))
    stopifnot(identical(read_fastq(output1),
        c("@read description MIRE{UMI:CG}", "AT", "+", "14")))
    succeeds(refine(list(action(seq_range(1L, 1L)))))
    stopifnot(identical(read_fastq(output1),
        c("@read description MIRE{UMI:A}", "CGT", "+", "234")))

    # Both open-bound forms, merged tags, and independent trimming of read ends.
    succeeds(refine(list(action(seq_range(end = 2L))),
                    list(action(seq_range(start = 3L))), paired = TRUE))
    stopifnot(identical(read_fastq(output1),
        c("@read description MIRE{UMI:ACCA}", "GT", "+", "34")))
    stopifnot(identical(read_fastq(output2),
        c("@read MIRE{UMI:ACCA}", "TG", "+", "56")))

    # Unsorted ranges are extracted in sequence order.
    ranges <- structure(list(seq_range(start = 4L), seq_range(end = 2L)),
                        class = "mire_seq_ranges")
    succeeds(refine(list(action(ranges, "mire_embed"))))
    stopifnot(identical(read_fastq(output1),
        c("@read description MIRE{UMI:ACT}", "ACGT", "+", "1234")))

    # Empty action lists remain valid; NULL still means no action was supplied.
    succeeds(refine(list()))
    stopifnot(identical(read_fastq(output1), read_fastq(input1)))
    fails(refine(NULL), "No sequence actions")

    # Gzip input and output exercise the same conversion and domain rules.
    compressed_input <- file.path(work, "read1.fq.gz")
    connection <- gzfile(compressed_input, "wt")
    writeLines(read_fastq(input1), connection)
    close(connection)
    succeeds(refine(list(action(seq_range(2L, 3L))), input = compressed_input,
                    output = output2))
    stopifnot(identical(read_fastq(output2),
        c("@read description MIRE{UMI:CG}", "AT", "+", "14")))

    # Malformed objects return argument-specific errors across the R boundary.
    for (start in list(0L, -1L, NA_integer_, integer(), c(1L, 2L))) {
        fails(refine(list(action(seq_range(start, 3L)))), "start must be a single positive integer")
    }
    fails(refine(list(action(seq_range(1, 3L)))), "start must be an integer")
    fails(refine(list(action(seq_range(3L, 1L)))), "greater than or equal")
    fails(refine(list(action(seq_range()))), "at least one")
    fails(refine(list(action(seq_range(1L, NA_integer_)))), "end must be a single positive integer")
    fails(refine(list(seq_range(1L, 2L))), "valid action class")
    fails(refine(list(action(seq_range(1L, 2L), tag = NULL))), "'tag' attribute")
    fails(refine(list(structure(list(1L), class = c("mire_trim", "mire_seq_range")))),
          "exactly 2 values")

    # The shared tag adapter rejects invalid tags before attempting file I/O.
    extract <- function(ranges) {
        .Call("wrap__koutput_reads", "unused", "unused", input1, NULL, output1,
              NULL, NULL, ranges, NULL, 1L, 1L, 1024L, 1L, 2L, 1L, PACKAGE = "mire")
    }
    fails(extract(list(seq_range(1L, 2L))), "valid tag class")
    overlap <- structure(list(seq_range(end = 3L), seq_range(start = 2L)),
                         class = c("mire_tag", "mire_seq_ranges"), tag = "UMI")
    valid_tag <- structure(seq_range(1L, 2L),
                           class = c("mire_tag", "mire_seq_range"), tag = "UMI")
    fails(extract(list(overlap, valid_tag)), "Overlapping ranges")
})
