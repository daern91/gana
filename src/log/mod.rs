use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

/// Initialize the tracing/logging subsystem.
///
/// When `to_file` is true, logs are written to a file in the OS temp directory.
/// Otherwise, logs go nowhere (useful for tests).
pub fn initialize(to_file: bool) {
    let builder = tracing_subscriber::fmt().with_env_filter(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    );

    if to_file {
        if let Some(path) = log_file_path() {
            if let Ok(file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = builder.with_writer(file).with_ansi(false).try_init();
                return;
            }
        }
    }

    // Fallback: discard output (test mode or file creation failed)
    let _ = builder
        .with_writer(std::io::sink)
        .with_ansi(false)
        .try_init();
}

/// Return the log file path: {temp_dir}/league.log
fn log_file_path() -> Option<PathBuf> {
    let mut path = std::env::temp_dir();
    path.push("league.log");
    Some(path)
}
