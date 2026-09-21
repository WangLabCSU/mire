#' Parse kraken report file
#'
#' @param kreport A string giving the path to a Kraken2 report file. Both the
#'   standard six-column format and the eight-column format produced with
#'   `--report-minimizer-data` are supported.
#' @param taxonomy An optional character vector of taxonomic groups to include,
#'   written as a taxid (`"562"`), a full major-rank name (`"Genus"`), a
#'   taxonomic level (`"G"`, `"G2"`), a level and scientific name
#'   (`"D__Bacteria"`), or a scientific name (`"Bacteria"`). Inputs are
#'   interpreted in that order; full major-rank names are case-sensitive.
#'   Matching taxa and all their descendants are retained. If
#'   `NULL`, all classified report rows, including the root row, are returned.
#'   Blank lines and unclassified (`U`) rows are skipped.
#' @return A data frame containing the report columns `percents`,
#'   `total_reads`, `reads`, `rank`, `taxid`, and `taxon`. Eight-column reports
#'   additionally contain `minimizer_len` and `minimizer_n_unique`. The list
#'   columns `ranks`, `taxids`, and `taxa` contain each row's ancestors only;
#'   the current taxon and root (`R`) are excluded. Intermediate ranks such as
#'   `R1` are retained. If no classified taxa match, the data frame has zero rows.
#' @details Taxids in report rows and numeric taxonomy selections must use
#'   decimal digits without leading zeros, for example `"2"` or `"562"`.
#'   A single `"0"` denotes unclassified reads. Empty identifiers, signs,
#'   whitespace, non-ASCII digits and leading zeros such as `"02"` or `"0002"`
#'   are rejected. There is no numeric upper bound; for example, `"4294967296"`
#'   is accepted.
#'
#'   A taxonomy selection with no intermediate level matches any depth at the
#'   same major rank: `"G__Genus group"` and `"G0__Genus group"` match that name
#'   at either `G` or `G2`. An intermediate level must match exactly:
#'   `"G2__Genus group"` matches that name at `G2`, but not at `G` or `G1`.
#'   Unrecognized levels or incomplete level-and-name conditions, such as
#'   `"G256"`, `"X__name"` or `"G__"`, are treated as complete scientific names.
#'   Empty selection labels and digit-only taxids with leading zeros remain invalid.
#' @seealso
#' <https://github.com/DerrickWood/kraken2/blob/master/docs/MANUAL.markdown>
#' @export
read_kreport <- function(kreport, taxonomy = NULL) {
    assert_string(kreport, allow_empty = FALSE)
    if (!is.null(taxonomy)) {
        taxonomy <- as.character(taxonomy)
        taxonomy <- taxonomy[!is.na(taxonomy)]
        if (length(taxonomy) == 0L) taxonomy <- NULL
    }
    out <- rust_call("read_kreport", kreport = kreport, taxonomy = taxonomy)
    class(out) <- "data.frame"
    attr(out, "row.names") <- .set_row_names(length(.subset2(out, 1L)))
    out
}
