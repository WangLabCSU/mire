Single-cell Analysis of Host-Microbiome Interactions
================

<!-- README.md is generated from README.Rmd. Please edit that file -->
<!-- badges: start -->

[![CRAN
status](https://www.r-pkg.org/badges/version/scmire)](https://CRAN.R-project.org/package=scmire)
[![Project Status: Active - The project has reached a stable, usable
state and is being actively
developed.](https://www.repostatus.org/badges/latest/active.svg)](https://www.repostatus.org/#active)
[![](https://cranlogs.r-pkg.org/badges/scmire)](https://cran.r-project.org/package=scmire)
<!-- badges: end -->

An integrated framework for high-resolution microbiome analysis in
single-cell sequencing. Leveraging Kraken2 for initial taxonomic
classification, the package performs barcode-aware filtering and
reconstructs read-level information by integrating cell barcodes and
UMIs. It quantifies k-mers and aggregates taxon-specific signals across
multiple taxonomic ranks. This enables accurate reconstruction and
quantitative profiling of microbial communities at single-cell
resolution.

## Installation

You can install `scmire` from `CRAN` using:

``` r
# install.packages("pak")
pak::pak("scmire")
```

Alternatively, install the development version from
[r-universe](https://yunuuuu.r-universe.dev/scmire) with:

``` r
pak::repo_add("https://yunuuuu.r-universe.dev")
pak::pak("scmire")
```

or from [GitHub](https://github.com/Yunuuuu/scmire) with:

``` r
pak::pak("Yunuuuu/scmire")
```

You must also install
[kraken2](https://github.com/DerrickWood/kraken2/wiki/Manual).

## sessionInfo

``` r
sessionInfo()
#> R version 4.4.2 (2024-10-31)
#> Platform: x86_64-pc-linux-gnu
#> Running under: Ubuntu 24.04.1 LTS
#> 
#> Matrix products: default
#> BLAS/LAPACK: /usr/lib/x86_64-linux-gnu/libmkl_rt.so;  LAPACK version 3.8.0
#> 
#> locale:
#>  [1] LC_CTYPE=C.UTF-8       LC_NUMERIC=C           LC_TIME=C.UTF-8       
#>  [4] LC_COLLATE=C.UTF-8     LC_MONETARY=C.UTF-8    LC_MESSAGES=C.UTF-8   
#>  [7] LC_PAPER=C.UTF-8       LC_NAME=C              LC_ADDRESS=C          
#> [10] LC_TELEPHONE=C         LC_MEASUREMENT=C.UTF-8 LC_IDENTIFICATION=C   
#> 
#> time zone: Asia/Shanghai
#> tzcode source: system (glibc)
#> 
#> attached base packages:
#> [1] stats     graphics  grDevices utils     datasets  methods   base     
#> 
#> loaded via a namespace (and not attached):
#>  [1] compiler_4.4.2    fastmap_1.2.0     cli_3.6.5         tools_4.4.2      
#>  [5] htmltools_0.5.8.1 yaml_2.3.10       rmarkdown_2.29    knitr_1.50       
#>  [9] xfun_0.52         digest_0.6.37     rlang_1.1.6       evaluate_1.0.3
```
