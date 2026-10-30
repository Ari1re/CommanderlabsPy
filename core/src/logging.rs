use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const LOG_DIR: &str = "./data/logs";
const LOG_FILE: &str = "./data/logs/commander-lab.log";
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

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

pub fn line(scope: &str, message: impl AsRef<str>) -> String {
    format!(
        "[{}] [{}] {}",
        timestamp(),
        scope,
        message.as_ref(),
    )
}

pub fn record(scope: &str, message: impl AsRef<str>) -> String {
    let formatted = line(scope, message);

    let _ = append_to_file(&formatted);
    formatted
}

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

pub fn log_path() -> &'static str {
    LOG_FILE
}
