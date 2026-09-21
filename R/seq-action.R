#' Define sequence range behavior: embed, trim, or both
#'
#' These functions define actions for embedding subsequences in FASTQ headers,
#' trimming subsequences, or both. Each action applies to a range or list of ranges.
#'
#' - `embed()` defines an action to copy the subsequence into the read header
#'   while retaining it in the read.
#' - `trim()` defines an action to remove the subsequence and its quality scores.
#' - `embed_trim()` defines an action to copy the subsequence into the header
#'   and remove it and its quality scores from the read.
#'
#' @param tag A non-empty character label for sequence content embedded in the
#'   FASTQ header. Required for `embed()` and `embed_trim()`, for example `"UMI"`
#'   or `"BARCODE"`.
#'
#' @param ranges A range or a list of ranges specifying the subsequence(s) to
#' process. Must be created using the [`seq_range()`] function.
#'
#' @return A sequence action containing the supplied ranges and, for embedding,
#' the tag label, with one of these classes:
#' \itemize{
#'   \item `mire_embed` — for embedding only
#'   \item `mire_trim` — for trimming only
#'   \item `mire_embed_trim` — for embedding and trimming
#' }
#' @name subseq_actions
#' @export
embed <- function(tag, ranges) {
    assert_string(tag, allow_empty = FALSE, allow_null = FALSE)
    UseMethod("embed", ranges)
}

#' @export
embed.mire_seq_range <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("mire_embed", "mire_seq_action", "mire_seq_range")
    )
}

#' @export
embed.mire_seq_ranges <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("mire_embed", "mire_seq_action", "mire_seq_ranges")
    )
}

#' @rdname subseq_actions
#' @export
trim <- function(ranges) UseMethod("trim", ranges)

#' @export
trim.mire_seq_range <- function(ranges) {
    structure(
        ranges,
        class = c("mire_trim", "mire_seq_action", "mire_seq_range")
    )
}

#' @export
trim.mire_seq_ranges <- function(ranges) {
    structure(
        ranges,
        class = c("mire_trim", "mire_seq_action", "mire_seq_ranges")
    )
}

#' @rdname subseq_actions
#' @export
embed_trim <- function(tag, ranges) {
    assert_string(tag, allow_empty = FALSE, allow_null = FALSE)
    UseMethod("embed_trim", ranges)
}

#' @export
embed_trim.mire_seq_range <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("mire_embed_trim", "mire_seq_action", "mire_seq_range")
    )
}

#' @export
embed_trim.mire_seq_ranges <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("mire_embed_trim", "mire_seq_action", "mire_seq_ranges")
    )
}

#' @export
c.mire_seq_action <- function(...) {
    cli::cli_abort(c(
        "Combining multiple {.cls mire_seq_action} objects with {.fn c} is not supported.",
        i = "Use {.fn list} to collect multiple actions instead."
    ))
}

is_action <- function(action) inherits(action, "mire_seq_action")
need_embed <- function(action) {
    inherits(action, c("mire_embed_trim", "mire_embed"))
}
