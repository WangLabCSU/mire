use std::fs::File;

pub(super) fn run<T>(
    path: &str,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(2000)
        .build()
        .map_err(|error| format!("cannot create profile guard: {error}"))?;
    let result = operation();
    if let Ok(report) = guard.report().build() {
        let file =
            File::create(path).map_err(|error| format!("Failed to create file {path}: {error}"))?;
        let mut options = pprof::flamegraph::Options::default();
        options.image_width = Some(2500);
        report
            .flamegraph_with_options(file, &mut options)
            .map_err(|error| format!("Failed to write flamegraph to {path}: {error}"))?;
    }
    result
}
