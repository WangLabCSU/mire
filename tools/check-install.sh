#!/bin/sh
# Exercise the R compiler/linker and source-package installation, not just Cargo.
# Run with: sh tools/check-install.sh [package-directory]
set -eu

package_dir=$(cd "${1:-.}" && pwd)
check_dir=$(mktemp -d "${TMPDIR:-/tmp}/mire-install.XXXXXX")
trap 'rm -rf "$check_dir"' EXIT
cd "$check_dir"

R CMD build --no-build-vignettes --no-manual "$package_dir"
mkdir library
R CMD INSTALL --preclean --library="$check_dir/library" ./mire_*.tar.gz

Rscript - "$check_dir/library" <<'RSCRIPT'
library_dir <- commandArgs(trailingOnly = TRUE)[[1L]]
.libPaths(c(library_dir, .libPaths()))
library(mire, lib.loc = library_dir)
stopifnot(normalizePath(find.package("mire")) == normalizePath(file.path(library_dir, "mire")))
report <- tempfile()
writeLines("100\t4\t2\tD\t2\tBacteria", report)
parsed <- mire::read_kreport(report)
stopifnot(is.data.frame(parsed), identical(parsed$taxid, "2"), identical(parsed$reads, 2))
writeLines("100\t4\t2\t30\t7\tD\t2\tBacteria", report)
parsed <- mire::read_kreport(report)
stopifnot(identical(parsed$minimizer_len, 30), identical(parsed$minimizer_n_unique, 7))
unlink(report)
cat("Source-package installation and public R-to-Rust calls passed.\n")
RSCRIPT
