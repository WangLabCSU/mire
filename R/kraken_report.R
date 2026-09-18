#' Parse kraken report file
#'
#' @param kreport A string giving the path to a Kraken2 report file. Both the
#'   standard six-column format and the eight-column format produced with
#'   `--report-minimizer-data` are supported.
#' @param taxonomy An optional character vector of taxonomic groups to include,
#'   written as a taxid or `"rank__name"`, for example `"562"` or
#'   `"D__Bacteria"`. Matching taxa and all their descendants are retained. If
#'   `NULL`, every report row, including the unclassified row, is returned.
#' @return A data frame containing the report columns `percents`,
#'   `total_reads`, `reads`, `rank`, `taxid`, and `taxon`. Eight-column reports
#'   additionally contain `minimizer_len` and `minimizer_n_unique`. The list
#'   columns `ranks`, `taxids`, and `taxa` contain each row's classified lineage;
#'   root and unclassified rows are not included as ancestors.
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
