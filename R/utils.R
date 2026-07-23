FASTQ_BATCH <- 256
KOUTPUT_BATCH <- 1000
CHUNK_BYTES <- 8L * 1024L * 1024L

is_scalar <- function(x) length(x) == 1L

dir_create <- function(path, ...) {
    if (!dir.exists(path) &&
        !dir.create(path = path, showWarnings = FALSE, ...)) {
        cli::cli_abort("Cannot create directory {.path {path}}")
    }
}

RUST_CALL <- .Call

#' @importFrom rlang caller_env
#' @keywords internal
rust_method <- function(class, method, ..., call = caller_env()) {
    rust_call(sprintf("%s__%s", class, method), ..., call = call)
}

#' @importFrom rlang caller_env
#' @keywords internal
rust_call <- function(.NAME, ..., call = caller_env()) {
    # call the function
    out <- RUST_CALL(sprintf("wrap__%s", .NAME), ...)

    # propagate error from rust --------------------
    if (!inherits(out, "extendr_result")) return(out) # styler: off
    if (!is.null(err <- .subset2(out, "err"))) {
        rlang::abort(err, call = call)
    }
    .subset2(out, "ok")
}
