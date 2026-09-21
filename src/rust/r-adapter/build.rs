use std::{env, error::Error, path::PathBuf, process::Command};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=R_HOME");
    println!("cargo:rerun-if-env-changed=PATH");

    // Native Unix Cargo tests must locate libR themselves; package installation
    // and cross-compilation continue to use the R toolchain's linker settings.
    if env::var("CARGO_CFG_TARGET_FAMILY").as_deref() != Ok("unix")
        || env::var("HOST")? != env::var("TARGET")?
    {
        return Ok(());
    }

    let r = env::var_os("R_HOME")
        .filter(|home| !home.is_empty())
        .map_or_else(
            || PathBuf::from("R"),
            |home| PathBuf::from(home).join("bin/R"),
        );
    // extendr-ffi 0.8.0 probes R without --vanilla, so startup messages can
    // corrupt its library path. Query the path without loading user profiles.
    let output = Command::new(r)
        .args(["--vanilla", "--slave", "-e", "cat(R.home('lib'))"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "Cannot determine the R library directory: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let library = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    if !library.is_dir() {
        return Err(format!("R library directory does not exist: {}", library.display()).into());
    }

    println!("cargo:rustc-link-search=native={}", library.display());
    // Cargo does not add external library directories to the test process's
    // loader path, so test executables also need a runtime search path.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", library.display());
    Ok(())
}
