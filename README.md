Microbiome Integrated Reconstruction and Estimation
================

<!-- README.md is generated from README.Rmd. Please edit that file -->

<!-- badges: start -->

[![Project Status: Active - The project has reached a stable, usable
state and is being actively
developed.](https://www.repostatus.org/badges/latest/active.svg)](https://www.repostatus.org/#active)
[![](https://cranlogs.r-pkg.org/badges/mire)](https://cran.r-project.org/package=mire)
<!-- badges: end -->

An integrated framework for microbiome reconstruction from sequencing
data. It leverages tools like Kraken2 for taxonomic classification and
combines cell barcodes, UMIs, and k-mer-based quantification to
reconstruct microbial signals. Designed for both bulk and single-cell
sequencing data, the package enables taxonomic and quantitative
profiling of microbial communities.

## Installation

Here, we use `pak` to install package, you can also use `remotes`:

``` r
if (!requireNamespace("pak")) {
    install.packages("pak",
        repos = sprintf(
            "https://r-lib.github.io/p/pak/devel/%s/%s/%s",
            .Platform$pkgType, R.Version()$os, R.Version()$arch
        )
    )
}
```

You can install the development version from
[r-universe](https://wanglabcsu.r-universe.dev/mire) with:

``` r
pak::repo_add("https://wanglabcsu.r-universe.dev")
pak::pak("mire")
```

or from [GitHub](https://github.com/WangLabCSU/mire) with:

``` r
pak::pak("WangLabCSU/mire")
```

You must also install
[kraken2](https://github.com/DerrickWood/kraken2/wiki/Manual).

## sessionInfo

``` r
sessionInfo()
#> R version 4.6.1 (2026-06-24)
#> Platform: x86_64-conda-linux-gnu
#> Running under: Ubuntu 26.04 LTS
#> 
#> Matrix products: default
#> BLAS/LAPACK: /home/yun/.local/share/pixi/envs/r-release/lib/libopenblasp-r0.3.33.so;  LAPACK version 3.12.0
#> 
#> locale:
#>  [1] LC_CTYPE=en_US.UTF-8       LC_NUMERIC=C              
#>  [3] LC_TIME=en_US.UTF-8        LC_COLLATE=en_US.UTF-8    
#>  [5] LC_MONETARY=en_US.UTF-8    LC_MESSAGES=en_US.UTF-8   
#>  [7] LC_PAPER=en_US.UTF-8       LC_NAME=C                 
#>  [9] LC_ADDRESS=C               LC_TELEPHONE=C            
#> [11] LC_MEASUREMENT=en_US.UTF-8 LC_IDENTIFICATION=C       
#> 
#> time zone: Asia/Shanghai
#> tzcode source: system (glibc)
#> 
#> attached base packages:
#> [1] stats     graphics  grDevices utils     datasets  methods   base     
#> 
#> loaded via a namespace (and not attached):
#>  [1] compiler_4.6.1  fastmap_1.2.0   cli_3.6.6       tools_4.6.1    
#>  [5] htmltools_0.5.9 otel_0.2.0      yaml_2.3.12     rmarkdown_2.31 
#>  [9] knitr_1.51      xfun_0.57       digest_0.6.39   rlang_1.3.0    
#> [13] evaluate_1.0.5
```
