.onAttach <- function(libname, pkgname) {
    version <- utils::packageDescription(pkgname, fields = "Version")
    msg <- paste0(
        "========================================\n",
        pkgname, " version ", version, "\n\n"
    )
    packageStartupMessage(msg)
}
