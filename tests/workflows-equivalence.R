# Compare the original and refactored shared libraries in separate R processes.
# Rscript --vanilla tests/workflows-equivalence.R OLD/mire.so NEW/mire.so [--strict]
args <- commandArgs(trailingOnly = TRUE)
strict <- "--strict" %in% args
args <- args[args != "--strict"]
if (length(args) == 0L) {
    message("Compatibility comparison requires paths to both original and refactored mire.so libraries.")
} else if (length(args) == 2L) {
    script <- sub("^--file=", "", grep("^--file=", commandArgs(), value = TRUE))
    snapshots <- vapply(seq_along(args), function(i) {
        output <- tempfile(fileext = ".rds")
        status <- system2(file.path(R.home("bin"), "Rscript"),
                          c("--vanilla", shQuote(script), "--snapshot", shQuote(normalizePath(args[i])), shQuote(output), if (strict) "--strict"),
                          timeout = 120L)
        stopifnot(status == 0L)
        output
    }, character(1))
    old <- readRDS(snapshots[1L]); new <- readRDS(snapshots[2L])
    unlink(snapshots)
    stopifnot(identical(names(old), names(new)))
    different <- names(old)[!vapply(names(old), function(name) identical(old[[name]], new[[name]]), logical(1))]
    if (length(different)) {
        for (name in different) {
            cat("DIFFERENCE:", name, "\n")
            print(old[[name]]); print(new[[name]])
        }
        stop(length(different), " of ", length(old), " compatibility cases differ")
    }
    cat(length(old), "compatibility cases match", if (strict) " (including order and error text)", "\n", sep = " ")
} else {
    stopifnot(length(args) == 3L, args[1L] == "--snapshot")
    dyn.load(normalizePath(args[2L]))
    local({
        work <- tempfile("mire-equivalence-"); dir.create(work)
        on.exit(unlink(work, recursive = TRUE))
        p <- function(name) file.path(work, name)
        results <- list()
        call <- function(name, ...) .Call(paste0("wrap__", name), ..., PACKAGE = "mire")
        record <- function(name, result, output = NULL, unordered = FALSE) {
            unordered <- unordered && !strict
            if (!is.null(result$err)) {
                # Strict member-move checks normalize only temporary fixture paths.
                value <- list(error = if (strict) gsub(work, "<fixture>", result$err, fixed = TRUE) else TRUE)
            } else if (!is.null(output)) {
                value <- lapply(output, function(path) {
                    if (!file.exists(path)) return(NULL)
                    con <- gzfile(path, "rt"); on.exit(close(con))
                    lines <- readLines(con)
                    if (unordered && endsWith(sub("[.]gz$", "", path), ".fq")) {
                        records <- if (length(lines)) vapply(split(lines, rep(seq_len(length(lines) / 4L), each = 4L)), paste, collapse = "\n", FUN.VALUE = character(1)) else character()
                        return(sort(unname(records)))
                    }
                    if (unordered) sort(lines) else lines
                })
            } else value <- result$ok
            results[[name]] <<- list(value) # preserve NULL results
            invisible(NULL)
        }
        write <- function(lines, name) {
            con <- if (endsWith(name, ".gz")) gzfile(p(name), "wt") else file(p(name), "wt")
            on.exit(close(con)); writeLines(lines, con)
        }
        report <- c("0\t0\t0\tU\t0\tunclassified", "100\t4\t0\tR\t1\troot",
                    "100\t4\t0\tD\t2\t  Bacteria", "100\t4\t0\tG\t10\t    Genus",
                    "50\t2\t2\tS\t11\t      Species A", "50\t2\t2\tS\t12\t      Species B")
        for (eight in c(FALSE, TRUE)) for (gzip in c(FALSE, TRUE)) {
            name <- paste0("report", if (eight) "8" else "6", if (gzip) ".gz" else ".tsv")
            write(if (eight) sub("\t([URDGS])\t", "\t20\t5\t\\1\t", report) else report, name)
            for (taxonomy in list(NULL, "G__Genus", "11", c("D__Bacteria", "S__Species A"))) {
                label <- paste(name, paste(taxonomy, collapse = "/"))
                record(label, call("read_kreport", p(name), taxonomy))
            }
        }
        repath <- p("report6.tsv")
        write(character(), "empty.tsv"); write("broken", "malformed.tsv")
        for (name in c("empty.tsv", "malformed.tsv", "absent.tsv"))
            record(paste("report-error", name), call("read_kreport", p(name), NULL))
        for (taxonomy in list("bad", character(), "99999"))
            record(paste("taxonomy-error", paste(taxonomy, collapse = "/")), call("read_kreport", repath, taxonomy))
        kraken <- c("C\ta\t11\t40\t11:10", "C\tb\tSpecies B (taxid 12)\t40\t12:10",
                    "C\tc\t99\t40\t99:10", "U\tu\t0\t40\t0:10")
        write(kraken, "kraken.tsv"); write(kraken, "kraken.tsv.gz")
        seq1 <- paste(rep("ACGT", 10), collapse = ""); seq2 <- paste(rep("GCTA", 10), collapse = "")
        qual <- paste(rep("I", 40), collapse = "")
        reads1 <- c("@a desc MIRE{UMI:AA}", seq1, "+", qual, "@b", seq2, "+", qual,
                    "@c", seq1, "+", qual, "@u", seq2, "+", qual)
        reads2 <- c("@a desc MIRE{UMI:TT}", seq2, "+", qual, "@b", seq1, "+", qual,
                    "@c", seq2, "+", qual, "@u", seq1, "+", qual)
        write(reads1, "r1.fq"); write(reads2, "r2.fq.gz")
        for (exclude in list(NULL, character(), "999", "2", "12")) for (desc in c(FALSE, TRUE)) {
            label <- paste("filter", paste(exclude, collapse = "/"), is.null(exclude), desc)
            record(label, call("kractor_koutput", repath, p("kraken.tsv.gz"), NULL, NULL, NULL, "11", exclude, desc,
                               p("filtered.tsv.gz"), 1L, 2L, 2048L, 1L, 1L), p("filtered.tsv.gz"))
        }
        for (kind in c("rank", "taxon", "taxonomy", "intersection", "root")) {
            record(paste("selection", kind), call("kractor_koutput", repath, p("kraken.tsv"),
                if (kind == "taxonomy") "G__Genus" else NULL,
                if (kind %in% c("rank", "intersection")) "S" else NULL,
                if (kind %in% c("taxon", "intersection")) "Species B" else NULL,
                if (kind == "root") "1" else NULL, NULL, TRUE,
                p("filtered.tsv"), 1L, 2L, 2048L, 1L, 1L), p("filtered.tsv"))
        }
        tag <- function(name) structure(list(NULL, 4L), tag = name, class = c("mire_tag", "mire_seq_range"))
        action <- function(kind, name = "BC") structure(list(1L, 4L), tag = name, class = c(paste0("mire_", kind), "mire_seq_range"))
        for (paired in c(FALSE, TRUE)) for (gzip in c(FALSE, TRUE)) for (threads in c(1L, 3L)) {
            out1 <- p(paste0("out1.fq", if (gzip) ".gz")); out2 <- p(paste0("out2.fq", if (gzip) ".gz"))
            output <- if (paired) c(out1, out2) else out1
            label <- paste(paired, gzip, threads)
            record(paste("extract", label), call("kractor_reads", p("kraken.tsv"), p("r1.fq"), out1,
                if (paired) p("r2.fq.gz") else NULL, if (paired) out2 else NULL,
                1L, 1L, 128L, 1L, threads), output, threads > 1L)
            for (kind in c("trim", "embed", "embed_trim")) {
                record(paste("refine", kind, label), call("seq_refine", p("r1.fq"), out1,
                    if (paired) p("r2.fq.gz") else NULL, if (paired) out2 else NULL,
                    list(action(kind)), if (paired) list(action(kind, "UMI")) else NULL,
                    1L, 128L, 1L, 1L, threads), output, threads > 1L)
            }
        }
        for (paired in c(FALSE, TRUE)) for (tags in c(FALSE, TRUE)) for (classification_pair in c(FALSE, TRUE)) {
            input <- if (classification_pair) paste(sub("40\t", "40:40\t", kraken), "|:| 11:10") else kraken
            write(input, "join-kraken.tsv")
            label <- paste(paired, tags, classification_pair)
            record(paste("join", label), call("koutput_reads", repath, p("join-kraken.tsv"), p("r1.fq"),
                if (paired) p("r2.fq.gz") else NULL, p("joined.tsv"), NULL, NULL,
                if (tags) list(tag("BARCODE"), tag("UMI")) else NULL,
                if (tags && paired) list(tag("BARCODE")) else NULL,
                2L, 2L, 2048L, 1L, 1L, 1L), p("joined.tsv"))
        }
        single <- paste("11", "NOTBC:xx BC:aa UMI:TT", "11:10", seq1, qual, sep = "\t")
        second <- paste("12", "BC:bb UMI:TT", "12:10", seq2, qual, sep = "\t")
        pair <- paste("11", "BC:aa UMI:TT", "11:10 |:| 12:10", paste(seq1, seq2), paste(qual, qual), sep = "\t")
        for (kind in c("single", "pair", "low-quality", "low-complexity", "empty", "bad")) {
            lines <- switch(kind, single = c(single, single, second), pair = pair,
                            `low-quality` = paste("11", "BC:aa UMI:TT", "11:10", seq1, paste0("!", substring(qual, 2L)), sep = "\t"),
                            `low-complexity` = paste("11", "BC:aa UMI:TT", "11:10", paste(rep("A", 40), collapse = ""), qual, sep = "\t"),
                            empty = character(), bad = "broken")
            write(lines, "count.tsv")
            for (barcode in list(NULL, "BC")) for (umi in list(NULL, "UMI")) {
                record(paste("count", kind, barcode, umi), call("krcount", p("count.tsv"), repath, "G__Genus", umi, barcode, 2L, 1L))
            }
        }
        # These formats were accepted without parsing unrelated classification fields.
        for (input in list(c("junk", "C\ta", "C\t", "C\tb\tignored"), character())) {
            write(input, "ids.tsv")
            record(paste("lenient-ids", length(input)), call("kractor_reads", p("ids.tsv"), p("r1.fq"), p("id-out.fq"),
                NULL, NULL, 1L, 2L, 2048L, 1L, 1L), p("id-out.fq"))
        }
        for (input in list("broken", "U\tmissing\t0\tinvalid\t0:10", "C\tmissing\t11\tinvalid\t11:10",
                           paste0(kraken[1L], "\textra"), character())) {
            write(input, "tolerated.tsv")
            label <- paste("tolerated", paste(input, collapse = "|"))
            record(paste("join", label), call("koutput_reads", repath, p("tolerated.tsv"), p("r1.fq"), NULL,
                p("tolerated-out.tsv"), NULL, NULL, NULL, NULL, 2L, 2L, 2048L, 1L, 1L, 1L), p("tolerated-out.tsv"))
            record(paste("filter", label), call("kractor_koutput", repath, p("tolerated.tsv"), NULL, NULL, NULL,
                "11", NULL, FALSE, p("tolerated-out.tsv"), 1L, 2L, 2048L, 1L, 1L), p("tolerated-out.tsv"))
        }
        for (batch in c(0L, 1L)) for (chunk in c(0L, 64L)) for (paired in c(FALSE, TRUE)) {
            record(paste("zero-sizing", batch, chunk, paired), call("kractor_reads", p("ids.tsv"), p("r1.fq"), p("zero1.fq"),
                if (paired) p("r2.fq.gz") else NULL, if (paired) p("zero2.fq") else NULL,
                1L, batch, chunk, 1L, 1L), if (paired) c(p("zero1.fq"), p("zero2.fq")) else p("zero1.fq"))
        }
        for (sequence in c("ACGT", "", "ACGTACGTACGTACGTACGT", seq1)) {
            write(paste("11", "", "11:1", sequence, strrep("I", nchar(sequence)), sep = "\t"), "short-count.tsv")
            record(paste("short-count", nchar(sequence)), call("krcount", p("short-count.tsv"), repath, NULL, NULL, NULL, 1L, 1L))
        }
        saveRDS(results, args[3L])
    })
}
