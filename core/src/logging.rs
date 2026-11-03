//! The app's log file (`data/logs/commander-lab.log`).
//!
//! How it fits: [`record`] appends a timestamped line under a scope
//! ("REF", "AI", "LLAMA"…). Generation and research log every source request
//! there, which is the first place to look when a source returns nothing.
//!
//! Things to work on:
//! - The log never rotates; cap its size (Settings → Storage shows it).

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const LOG_DIR: &str = "./data/logs";
const LOG_FILE: &str = "./data/logs/commander-lab.log";
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// "seconds.millis" since 1970: easy to sort and diff, not human dates.
pub fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    format!(
        "{}.{:03}",
        now.as_secs(),
        now.subsec_millis(),
    )
}

/// Format a log line, "[ts] [SCOPE] message", without writing it.
pub fn line(scope: &str, message: impl AsRef<str>) -> String {
    format!(
        "[{}] [{}] {}",
        timestamp(),
        scope,
        message.as_ref(),
    )
}

/// Write a line to the log file and return it (callers often also push it to
/// a job log). File errors are ignored, since logging must never break a build.
pub fn record(scope: &str, message: impl AsRef<str>) -> String {
    let formatted = line(scope, message);

    let _ = append_to_file(&formatted);
    formatted
}

/// Append one line, rotating first if the file is too big. The file is opened
/// on every call; fine at our volume, but a buffered writer would be faster.
fn append_to_file(line: &str) -> std::io::Result<()> {
    fs::create_dir_all(LOG_DIR)?;
    rotate_if_needed()?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)?;

    writeln!(file, "{line}")?;
    Ok(())
}

/// Past 5 MB, rename the log to commander-lab.previous.log (replacing the
/// older one), so at most about 10 MB is kept.
fn rotate_if_needed() -> std::io::Result<()> {
    let path = Path::new(LOG_FILE);

    let Ok(metadata) = path.metadata() else {
        return Ok(());
    };

    if metadata.len() < MAX_LOG_BYTES {
        return Ok(());
    }

    let old = Path::new(LOG_DIR).join("commander-lab.previous.log");

    if old.exists() {
        let _ = fs::remove_file(&old);
    }

    fs::rename(path, old)?;
    Ok(())
}

/// Where the log lives, shown in Settings → Storage and bug reports.
pub fn log_path() -> &'static str {
    LOG_FILE
}
