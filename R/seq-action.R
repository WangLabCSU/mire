#' Define sequence range behavior: embed, trim, or both
#'
#' These functions annotate a range or a list of ranges to indicate how
#' the specified subsequences (e.g. UMI or barcode) should be handled during
#' FASTQ processing.
#'
#' - `embed()` extracts the sequence and appends it to the read header without
#'   modifying the sequence.
#' - `trim()` removes the specified subsequence from the read but does not store
#'   it.
#' - `embed_trim()` both extracts the subsequence for the header and removes it
#'   from the read.
#'
#' Each function wraps the input range object in a new class to indicate its
#' behavior downstream.
#'
#' @param tag An character label used when embedding sequence content
#'   into the FASTQ header (used with `embed()` and `embed_trim()`).
#'
#'   For UMI and barcode actions, the tag will be assigned automatically as
#'   `"UMI"` and `"BARCODE"` respectively.
#'
#'   For other types of actions, you must explicitly specify a `tag` to ensure
#'   clarity in the embedded header.
#'
#' @param ranges A range or a list of ranges specifying the subsequence(s) to
#' process. Must be created using the [`seq_range()`] function.
#'
#' @return An annotated `scmire_seq_range` or `scmire_seq_ranges` object. object
#' with behavior-specific class:
#' \itemize{
#'   \item `scmire_embed` — for embedding only
#'   \item `scmire_trim` — for trimming only
#'   \item `scmire_embed_trim` — for embedding and trimming
#' }
#' @name subseq_actions
#' @export
embed <- function(tag, ranges) {
    assert_string(tag, allow_empty = FALSE, allow_null = FALSE)
    UseMethod("embed", ranges)
}

#' @export
embed.scmire_seq_range <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("scmire_embed", "scmire_seq_action", "scmire_seq_range")
    )
}

#' @export
embed.scmire_seq_ranges <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("scmire_embed", "scmire_seq_action", "scmire_seq_ranges")
    )
}

#' @rdname subseq_actions
#' @export
trim <- function(ranges) UseMethod("trim", ranges)

#' @export
trim.scmire_seq_range <- function(ranges) {
    structure(
        ranges,
        class = c("scmire_trim", "scmire_seq_action", "scmire_seq_range")
    )
}

#' @export
trim.scmire_seq_ranges <- function(ranges) {
    structure(
        ranges,
        class = c("scmire_trim", "scmire_seq_action", "scmire_seq_ranges")
    )
}

#' @rdname subseq_actions
#' @export
embed_trim <- function(tag, ranges) {
    assert_string(tag, allow_empty = FALSE, allow_null = FALSE)
    UseMethod("embed_trim", ranges)
}

#' @export
embed_trim.scmire_seq_range <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("scmire_embed_trim", "scmire_seq_action", "scmire_seq_range")
    )
}

#' @export
embed_trim.scmire_seq_ranges <- function(tag, ranges) {
    structure(
        ranges,
        tag = tag,
        class = c("scmire_embed_trim", "scmire_seq_action", "scmire_seq_ranges")
    )
}

#' @export
c.scmire_seq_action <- function(...) {
    cli::cli_abort(c(
        "Combining multiple {.cls scmire_seq_action} objects with {.fn c} is not supported.",
        i = "Use {.fn list} to collect multiple actions instead."
    ))
}

is_action <- function(action) inherits(action, "scmire_seq_action")
need_embed <- function(action) {
    inherits(action, c("scmire_embed_trim", "scmire_embed"))
}
